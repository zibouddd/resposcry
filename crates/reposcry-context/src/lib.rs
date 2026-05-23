use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::debug;

use reposcry_cache::db::CacheDb;
use reposcry_git::GitIntegration;
use reposcry_graph::edge::EdgeKind;
use reposcry_graph::graph::CodeGraph;
use reposcry_graph::node::NodeKind;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPack {
    pub user_task: String,
    pub relevant_files: Vec<ContextFile>,
    pub dependency_paths: Vec<String>,
    pub reverse_dependencies: Vec<ReverseDep>,
    pub risk_warnings: Vec<String>,
    pub suggested_read_order: Vec<String>,
    pub suggested_tests: Vec<String>,
    pub architecture_rules: Vec<String>,
    pub confidence: Confidence,
    pub strict_warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextFile {
    pub path: String,
    pub reason: String,
    pub important_symbols: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReverseDep {
    pub path: String,
    pub used_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    pub token_budget: u32,
    pub strict_mode: bool,
    pub max_files: u32,
    pub max_reverse_depth: u32,
    pub include_full_files: bool,
    pub format: OutputFormat,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            token_budget: 20_000,
            strict_mode: false,
            max_files: 30,
            max_reverse_depth: 2,
            include_full_files: false,
            format: OutputFormat::Human,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputFormat {
    Human,
    Markdown,
    Json,
}

impl Default for OutputFormat {
    fn default() -> Self {
        Self::Markdown
    }
}

pub struct ContextBuilder {
    graph: CodeGraph,
    cache: Option<CacheDb>,
    git: Option<GitIntegration>,
    config: ContextConfig,
}

impl ContextBuilder {
    pub fn new(graph: CodeGraph, config: ContextConfig) -> Self {
        Self {
            graph,
            cache: None,
            git: None,
            config,
        }
    }

    pub fn with_cache(mut self, cache: CacheDb) -> Self {
        self.cache = Some(cache);
        self
    }

    pub fn with_git(mut self, git: GitIntegration) -> Self {
        self.git = Some(git);
        self
    }

    pub fn build(&self, task: &str) -> Result<ContextPack> {
        debug!("Building context pack for task: {}", task);
        let keywords = extract_keywords(task);

        // Step 1: Search files/symbols by keywords
        let mut matched_files: Vec<ContextFile> = Vec::new();
        for node in self.graph.nodes.values() {
            let path = match &node.file_path {
                Some(p) => p,
                None => continue,
            };
            let relevance = score_relevance(&node.name, path, &keywords);
            if relevance > 0.0 {
                matched_files.push(ContextFile {
                    path: path.to_string(),
                    reason: format!("Keyword match (score: {:.2})", relevance),
                    important_symbols: self.symbols_in_file(path),
                });
            }
        }
        // Also search by keyword in file content via cache
        if let Some(ref cache) = self.cache {
            for entry in cache.get_all_files()? {
                if matched_files.iter().any(|f| f.path == entry.path) {
                    continue;
                }
                let relevance = score_relevance(&entry.path, &entry.path, &keywords);
                if relevance > 0.0 {
                    matched_files.push(ContextFile {
                        path: entry.path.clone(),
                        reason: format!("File name match (score: {:.2})", relevance),
                        important_symbols: self.symbols_in_file(&entry.path),
                    });
                }
            }
        }
        // Sort by relevance
        matched_files.sort_by(|a, b| {
            let a_score = score_relevance(&a.path, &a.path, &keywords);
            let b_score = score_relevance(&b.path, &b.path, &keywords);
            b_score
                .partial_cmp(&a_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        matched_files.dedup_by(|a, b| a.path == b.path);
        let max_by_budget = (self.config.token_budget / 750).clamp(4, self.config.max_files);
        matched_files.truncate(max_by_budget as usize);

        // Step 2-3: Find dependency paths and reverse deps
        let dependency_paths = self.find_dependency_paths(&matched_files);
        let reverse_dependencies = self.find_reverse_dependencies(&matched_files);

        // Step 4: Find tests
        let suggested_tests = self.find_tests(&matched_files);

        // Step 5: Risk warnings
        let risk_warnings = self.compute_risk_warnings(&matched_files);

        // Step 6: Architecture rules
        let architecture_rules = vec![
            "UI components should not directly query database.".into(),
            "Filter state should be serializable.".into(),
            "Backend defaults should not contradict frontend empty-state.".into(),
        ];

        // Step 7: Determine confidence
        let confidence = if matched_files.is_empty() {
            Confidence::Low
        } else if matched_files.len() < 3 {
            Confidence::Medium
        } else {
            Confidence::High
        };

        // Step 8: Strict mode check
        let mut strict_warnings = Vec::new();
        if self.config.strict_mode {
            if matched_files.is_empty() {
                strict_warnings.push("No relevant source files found.".into());
            }
            if dependency_paths.is_empty() {
                strict_warnings.push("No dependency paths found.".into());
            }
            if reverse_dependencies.is_empty() {
                strict_warnings.push("No reverse dependency check performed.".into());
            }
            if suggested_tests.is_empty() {
                strict_warnings.push("No test candidates found. Consider adding tests.".into());
            }
            if confidence == Confidence::Low {
                strict_warnings.push(format!(
                    "Confidence is LOW. Consider searching for: {}",
                    keywords.join(", ")
                ));
            }
        }

        // Build read order
        let mut suggested_read_order: Vec<String> =
            matched_files.iter().map(|f| f.path.clone()).collect();
        if suggested_read_order.len() > 10 {
            suggested_read_order.truncate(10);
        }

        Ok(ContextPack {
            user_task: task.to_string(),
            relevant_files: matched_files,
            dependency_paths,
            reverse_dependencies,
            risk_warnings,
            suggested_read_order,
            suggested_tests,
            architecture_rules,
            confidence,
            strict_warnings,
        })
    }

    fn symbols_in_file(&self, path: &str) -> Vec<String> {
        self.graph
            .nodes
            .values()
            .filter(|n| n.file_path.as_deref() == Some(path) && n.kind != NodeKind::File)
            .map(|n| {
                if let Some(signature) = &n.signature {
                    format!("{} — {}", n.name, signature)
                } else {
                    n.name.clone()
                }
            })
            .collect()
    }

    fn find_dependency_paths(&self, files: &[ContextFile]) -> Vec<String> {
        let mut paths = Vec::new();
        for file in files {
            let mut deps: Vec<String> = self
                .graph
                .edges
                .iter()
                .filter(|e| e.kind == EdgeKind::Imports)
                .filter(|e| {
                    self.graph
                        .nodes
                        .get(&e.source_id)
                        .and_then(|n| n.file_path.as_deref())
                        == Some(&file.path)
                })
                .filter_map(|e| {
                    self.graph
                        .nodes
                        .get(&e.target_id)
                        .and_then(|n| n.file_path.clone())
                })
                .collect();
            deps.sort();
            deps.dedup();
            if !deps.is_empty() {
                let chain = format!("{} → {}", file.path, deps.join(" → "));
                paths.push(chain);
            }
        }
        paths
    }

    fn find_reverse_dependencies(&self, files: &[ContextFile]) -> Vec<ReverseDep> {
        let mut results = Vec::new();
        for file in files {
            let mut users: Vec<String> = self
                .graph
                .edges
                .iter()
                .filter(|e| e.kind == EdgeKind::Imports)
                .filter(|e| {
                    self.graph
                        .nodes
                        .get(&e.target_id)
                        .and_then(|n| n.file_path.as_deref())
                        == Some(&file.path)
                })
                .filter_map(|e| {
                    self.graph
                        .nodes
                        .get(&e.source_id)
                        .and_then(|n| n.file_path.clone())
                })
                .collect();
            users.sort();
            users.dedup();
            if !users.is_empty() {
                results.push(ReverseDep {
                    path: file.path.clone(),
                    used_by: users,
                });
            }
        }
        results
    }

    fn find_tests(&self, ctx_files: &[ContextFile]) -> Vec<String> {
        let _ = ctx_files;
        let mut tests: Vec<String> = self
            .graph
            .nodes
            .values()
            .filter(|n| {
                n.kind == NodeKind::Test
                    && n.file_path.as_deref().map_or(false, |p| p.contains("test"))
            })
            .filter_map(|n| n.file_path.clone())
            .collect();
        tests.sort();
        tests.dedup();
        tests
    }

    fn compute_risk_warnings(&self, files: &[ContextFile]) -> Vec<String> {
        let mut warnings = Vec::new();
        for file in files {
            let deps = self
                .graph
                .edges
                .iter()
                .filter(|e| e.kind == EdgeKind::Imports)
                .filter(|e| {
                    self.graph
                        .nodes
                        .get(&e.source_id)
                        .and_then(|n| n.file_path.as_deref())
                        == Some(&file.path)
                })
                .count();
            let rdeps = self
                .graph
                .edges
                .iter()
                .filter(|e| e.kind == EdgeKind::Imports)
                .filter(|e| {
                    self.graph
                        .nodes
                        .get(&e.target_id)
                        .and_then(|n| n.file_path.as_deref())
                        == Some(&file.path)
                })
                .count();
            if rdeps > 5 {
                warnings.push(format!(
                    "`{}` has high fan-in ({} dependents). Changes may have wide impact.",
                    file.path, rdeps
                ));
            }
            if deps > 10 {
                warnings.push(format!(
                    "`{}` has high fan-out ({} dependencies). May be doing too much.",
                    file.path, deps
                ));
            }
        }
        warnings
    }

    pub fn render_markdown(&self, pack: &ContextPack) -> String {
        let mut md = String::new();
        md.push_str("# AI Context Pack\n\n");
        md.push_str(&format!("## User task\n\n{}\n\n", pack.user_task));
        md.push_str("## Relevant files\n\n");
        for file in &pack.relevant_files {
            md.push_str(&format!("### {}\n", file.path));
            md.push_str(&format!("Reason: {}\n", file.reason));
            if !file.important_symbols.is_empty() {
                md.push_str("Important symbols:\n");
                for sym in &file.important_symbols {
                    md.push_str(&format!("- {}\n", sym));
                }
            }
            md.push('\n');
        }
        if !pack.dependency_paths.is_empty() {
            md.push_str("## Dependency paths\n\n");
            for path in &pack.dependency_paths {
                md.push_str(&format!("{}\n", path));
            }
            md.push('\n');
        }
        if !pack.reverse_dependencies.is_empty() {
            md.push_str("## Reverse dependencies\n\n");
            for rd in &pack.reverse_dependencies {
                md.push_str(&format!("{} is used by:\n", rd.path));
                for user in &rd.used_by {
                    md.push_str(&format!("- {}\n", user));
                }
            }
            md.push('\n');
        }
        if !pack.risk_warnings.is_empty() {
            md.push_str("## Risk warnings\n\n");
            for warning in &pack.risk_warnings {
                md.push_str(&format!("- {}\n", warning));
            }
            md.push('\n');
        }
        md.push_str(&format!("## Suggested files to read before editing\n\n"));
        for (i, file) in pack.suggested_read_order.iter().enumerate() {
            md.push_str(&format!("{}. {}\n", i + 1, file));
        }
        md.push('\n');
        if !pack.suggested_tests.is_empty() {
            md.push_str("## Suggested tests\n\n");
            for test in &pack.suggested_tests {
                md.push_str(&format!("- {}\n", test));
            }
            md.push('\n');
        }
        md.push_str(&format!(
            "## Confidence\n\n{}\n\n",
            match pack.confidence {
                Confidence::High => "HIGH ✓",
                Confidence::Medium => "MEDIUM ⚠",
                Confidence::Low => "LOW ✗",
            }
        ));
        if !pack.strict_warnings.is_empty() {
            md.push_str("## Strict mode warnings\n\n");
            for warning in &pack.strict_warnings {
                md.push_str(&format!("- {}\n", warning));
            }
            md.push('\n');
        }
        if !pack.architecture_rules.is_empty() {
            md.push_str("## Architecture rules\n\n");
            for rule in &pack.architecture_rules {
                md.push_str(&format!("- {}\n", rule));
            }
            md.push('\n');
        }
        md
    }
}

fn extract_keywords(task: &str) -> Vec<String> {
    let stop_words = [
        "the",
        "a",
        "an",
        "is",
        "are",
        "was",
        "were",
        "be",
        "been",
        "being",
        "have",
        "has",
        "had",
        "do",
        "does",
        "did",
        "will",
        "would",
        "could",
        "should",
        "may",
        "might",
        "shall",
        "can",
        "fix",
        "add",
        "change",
        "update",
        "remove",
        "implement",
        "make",
        "get",
        "set",
        "this",
        "that",
        "these",
        "those",
        "to",
        "of",
        "in",
        "for",
        "on",
        "with",
        "at",
        "by",
        "from",
        "as",
        "into",
        "about",
        "after",
        "before",
        "between",
        "under",
        "and",
        "but",
        "or",
        "nor",
        "not",
        "so",
        "yet",
        "no",
        "bug",
        "feature",
        "issue",
        "task",
        "when",
        "no",
        "not",
    ];
    task.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .filter(|w| !stop_words.contains(w))
        .map(|w| w.to_lowercase())
        .collect()
}

fn score_relevance(name: &str, path: &str, keywords: &[String]) -> f64 {
    let mut score = 0.0;
    let lower_name = name.to_lowercase();
    let lower_path = path.to_lowercase();
    for kw in keywords {
        let kw_lower = kw.to_lowercase();
        if lower_name.contains(&kw_lower) {
            score += 1.0;
        }
        if lower_path.contains(&kw_lower) {
            score += 0.5;
        }
    }
    score
}
