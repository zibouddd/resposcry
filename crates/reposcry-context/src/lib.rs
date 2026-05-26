use std::collections::HashSet;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::debug;

use reposcry_cache::db::CacheDb;
use reposcry_git::GitIntegration;
use reposcry_graph::edge::EdgeKind;
use reposcry_graph::graph::CodeGraph;
use reposcry_graph::node::{GraphNode, NodeKind};

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
    pub implementation_plan: Vec<String>,
    pub confidence: Confidence,
    pub strict_warnings: Vec<String>,
    pub omitted_symbols: Vec<OmittedSymbols>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OmittedSymbols {
    pub path: String,
    pub omitted_count: u32,
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
    pub max_symbols_per_file: u32,
    pub show_omitted: bool,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            token_budget: 4_000,
            strict_mode: false,
            max_files: 30,
            max_reverse_depth: 2,
            include_full_files: false,
            format: OutputFormat::Human,
            max_symbols_per_file: 8,
            show_omitted: false,
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
        let backend_keywords = [
            "backend", "storage", "database", "cache", "provider", "driver", "adapter",
            "plugin", "feature",
        ];
        let task_lower = task.to_lowercase();
        let is_backend_task = backend_keywords.iter().any(|kw| task_lower.contains(kw));

        // Step 1: Search files/symbols by keywords. This intentionally allows multiple hits
        // per file; we dedupe by path after ranking so the strongest duplicate survives.
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
                    important_symbols: self.symbols_in_file(path, &keywords),
                });
            }
        }

        // Also search by filename via cache. Cache entries are file-level, so this catches
        // manifests and public crate roots that may not have relevant symbols.
        if let Some(ref cache) = self.cache {
            for entry in cache.get_all_files()? {
                if matched_files.iter().any(|f| f.path == entry.path) {
                    continue;
                }
                let relevance = score_file_relevance(&entry.path, &keywords, is_backend_task);
                if relevance > 0.0 {
                    matched_files.push(ContextFile {
                        path: entry.path.clone(),
                        reason: format!("File name match (score: {:.2})", relevance),
                        important_symbols: self.symbols_in_file(&entry.path, &keywords),
                    });
                }
            }
        }

        // Step 1b: Force-include manifests and public module roots for backend/storage tasks.
        if is_backend_task {
            if let Some(ref cache) = self.cache {
                for entry in cache.get_all_files()? {
                    let path = normalize_path(&entry.path);
                    let is_manifest = path.ends_with("cargo.toml")
                        || path.ends_with("package.json")
                        || path.ends_with("/lib.rs")
                        || path.ends_with("/mod.rs");
                    if is_manifest && !matched_files.iter().any(|f| f.path == entry.path) {
                        let relevance = score_file_relevance(&entry.path, &keywords, true);
                        if relevance > 0.0 {
                            matched_files.push(ContextFile {
                                path: entry.path.clone(),
                                reason: "Manifest/public API for backend/storage task".into(),
                                important_symbols: self.symbols_in_file(&entry.path, &keywords),
                            });
                        }
                    }
                }
            }
        }

        // Sort by task relevance, downranking fixtures and generated/benchmark context.
        matched_files.sort_by(|a, b| {
            let a_score = score_file_relevance(&a.path, &keywords, is_backend_task);
            let b_score = score_file_relevance(&b.path, &keywords, is_backend_task);
            b_score
                .partial_cmp(&a_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.path.cmp(&b.path))
        });

        // Dedupe before budget truncation. This avoids spending file slots on repeated db.rs
        // hits and lets compact budgets keep Cargo.toml/lib.rs/test-adjacent context.
        let mut seen_file_paths = HashSet::new();
        matched_files.retain(|f| seen_file_paths.insert(f.path.clone()));

        // Track omitted symbols per file after edit-relevance ranking.
        let max_syms = self.config.max_symbols_per_file as usize;
        let mut omitted_list = Vec::new();
        for file in &mut matched_files {
            if file.important_symbols.len() > max_syms {
                let omitted_count = file.important_symbols.len() as u32 - max_syms as u32;
                file.important_symbols.truncate(max_syms);
                omitted_list.push(OmittedSymbols {
                    path: file.path.clone(),
                    omitted_count,
                });
            }
        }

        let max_by_budget = (self.config.token_budget / 750).clamp(4, self.config.max_files);
        matched_files.truncate(max_by_budget as usize);

        // Step 2-3: Find dependency paths and reverse deps
        let dependency_paths = self.find_dependency_paths(&matched_files);
        let reverse_dependencies = self.find_reverse_dependencies(&matched_files);

        // Step 4: Find tests
        let suggested_tests = self.find_tests(&matched_files);

        // Step 5: Risk warnings
        let risk_warnings = self.compute_risk_warnings(&matched_files);

        // Step 6: Architecture rules and implementation plan — filter by matched file paths
        let matched_paths: HashSet<&str> = matched_files.iter().map(|f| f.path.as_str()).collect();
        let architecture_rules = self.compute_architecture_rules(&matched_paths);
        let implementation_plan = self.compute_implementation_plan(task, &matched_files);

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

        // Build read order (already deduped relevant files, keep ranking order)
        let mut suggested_read_order: Vec<String> =
            matched_files.iter().map(|f| f.path.clone()).collect();
        suggested_read_order.truncate(10);

        let mut pack = ContextPack {
            user_task: task.to_string(),
            relevant_files: matched_files,
            dependency_paths,
            reverse_dependencies,
            risk_warnings,
            suggested_read_order,
            suggested_tests,
            architecture_rules,
            implementation_plan,
            confidence,
            strict_warnings,
            omitted_symbols: if self.config.show_omitted { omitted_list } else { vec![] },
        };
        pack.dedupe();
        Ok(pack)
    }

    fn symbols_in_file(&self, path: &str, keywords: &[String]) -> Vec<String> {
        let mut nodes: Vec<&GraphNode> = self
            .graph
            .nodes
            .values()
            .filter(|n| n.file_path.as_deref() == Some(path) && n.kind != NodeKind::File)
            .collect();

        nodes.sort_by(|a, b| {
            let a_score = symbol_relevance_score(a, keywords);
            let b_score = symbol_relevance_score(b, keywords);
            b_score
                .partial_cmp(&a_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.name.cmp(&b.name))
        });

        nodes.iter().map(|n| Self::format_symbol(n)).collect()
    }

    fn format_symbol(node: &GraphNode) -> String {
        let line_range = match (node.start_line, node.end_line) {
            (Some(start), Some(end)) if end > start => format!(" L{}-{}", start, end),
            (Some(start), _) => format!(" L{}", start),
            _ => String::new(),
        };

        if let Some(signature) = &node.signature {
            format!(
                "{}{} - {}",
                node.name,
                line_range,
                Self::ascii_arrow(signature)
            )
        } else {
            format!("{}{} ({})", node.name, line_range, node.kind.as_str())
        }
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
                let chain = format!("{} -> {}", file.path, deps.join(" -> "));
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
        let matched_crates: HashSet<String> = ctx_files
            .iter()
            .filter_map(|f| {
                normalize_path(&f.path)
                    .split('/')
                    .find(|segment| segment.starts_with("reposcry-"))
                    .map(|c| c.to_string())
            })
            .collect();

        let mut tests = Vec::new();
        let mut crate_names: Vec<String> = matched_crates.iter().cloned().collect();
        crate_names.sort();
        for crate_name in &crate_names {
            tests.push(format!("cargo test -p {}", crate_name));
        }

        let mut graph_tests: Vec<(String, u32)> = self
            .graph
            .nodes
            .values()
            .filter(|n| {
                n.kind == NodeKind::Test
                    && n.file_path.as_deref().map_or(false, |p| p.contains("test"))
            })
            .filter_map(|n| {
                let path = n.file_path.clone()?;
                let normalized = normalize_path(&path);
                let crate_match = matched_crates.iter().find(|c| normalized.contains(c.as_str()));
                let priority: u32 = if crate_match.is_some() {
                    0 // same crate = highest priority
                } else if normalized.contains("fixtures") || normalized.contains("benchmarks") {
                    2 // fixture tests = lowest priority
                } else {
                    1 // other crate tests
                };
                Some((path, priority))
            })
            .collect();
        graph_tests.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));

        for (path, priority) in graph_tests {
            if priority > 0 && !tests.is_empty() {
                continue;
            }
            tests.push(path);
            if tests.len() >= 8 {
                break;
            }
        }

        let mut seen_tests = HashSet::new();
        tests.retain(|t| seen_tests.insert(t.clone()));
        tests
    }

    fn compute_risk_warnings(&self, files: &[ContextFile]) -> Vec<String> {
        let mut seen_paths = HashSet::new();
        let mut warnings = Vec::new();
        for file in files {
            if !seen_paths.insert(file.path.clone()) {
                continue;
            }
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
            if deps > 10 {
                warnings.push(format!(
                    "`{}` has high fan-out ({} dependencies). May be doing too much.",
                    file.path, deps
                ));
            }
        }
        warnings
    }

    fn compute_architecture_rules(&self, matched_paths: &HashSet<&str>) -> Vec<String> {
        let mut rules = Vec::new();

        let has_cache_files = matched_paths.iter().any(|p| p.contains("reposcry-cache"));
        if has_cache_files {
            rules.push("Cache backend changes must preserve existing SQLite CacheDb behavior.".into());
            rules.push("New backends must maintain search index, import, symbol, and call-site semantics.".into());
        }

        let has_cli_files = matched_paths.iter().any(|p| p.contains("reposcry-cli"));
        if has_cli_files {
            rules.push("CLI commands should prefer the crg_cli module for complex output formatting.".into());
        }

        let has_context_files = matched_paths
            .iter()
            .any(|p| p.contains("reposcry-context") || p.contains("reposcry-graph"));
        if has_context_files {
            rules.push("Context pack changes must preserve backward compatibility of the ContextPack JSON schema.".into());
        }

        rules
    }

    fn compute_implementation_plan(&self, task: &str, files: &[ContextFile]) -> Vec<String> {
        let task_lower = task.to_lowercase();
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        let cache_backend_task = task_lower.contains("cache")
            && (task_lower.contains("backend")
                || task_lower.contains("storage")
                || task_lower.contains("database")
                || task_lower.contains("adapter"));

        if cache_backend_task && paths.iter().any(|p| p.contains("reposcry-cache")) {
            return vec![
                "Inspect crates/reposcry-cache/src/lib.rs and crates/reposcry-cache/Cargo.toml before changing the storage API.".into(),
                "Keep CacheDb as the current SQLite-backed implementation unless introducing a trait preserves all existing public methods.".into(),
                "Touch constructor/config paths first: CacheDb::open, CacheDb::open_in_memory, CacheDb::initialize, set_config, and get_config.".into(),
                "Preserve search document/vector, import, symbol, call-site, and edge persistence semantics for the new backend.".into(),
                "Run cargo test -p reposcry-cache before dependent CLI/context tests.".into(),
            ];
        }

        Vec::new()
    }

    fn ascii_arrow(s: &str) -> String {
        s.replace('\u{2014}', "-")
            .replace('\u{2013}', "-")
            .replace('\u{2192}', "->")
            .replace('\u{2190}', "<-")
            .replace('\u{2713}', "[ok]")
            .replace('\u{2717}', "[no]")
            .replace('\u{26a0}', "[!]")
            .replace('\u{2018}', "'")
            .replace('\u{2019}', "'")
            .replace('\u{201c}', "\"")
            .replace('\u{201d}', "\"")
    }

    pub fn render_markdown(&self, pack: &ContextPack) -> String {
        let mut md = String::new();
        md.push_str("# AI Context Pack\n\n");
        md.push_str("## User task\n\n");
        md.push_str(&Self::ascii_arrow(&pack.user_task));
        md.push_str("\n\n");
        md.push_str("## Relevant files\n\n");
        for file in &pack.relevant_files {
            md.push_str(&format!("### {}\n", file.path));
            md.push_str(&format!("Reason: {}\n", Self::ascii_arrow(&file.reason)));
            if !file.important_symbols.is_empty() {
                md.push_str("Important symbols:\n");
                for sym in &file.important_symbols {
                    md.push_str(&format!("- {}\n", Self::ascii_arrow(sym)));
                }
            }
            md.push('\n');
        }
        if !pack.omitted_symbols.is_empty() {
            md.push_str("## Omitted lower-priority symbols\n\n");
            for omitted in &pack.omitted_symbols {
                md.push_str(&format!(
                    "- {}: {} omitted\n",
                    omitted.path, omitted.omitted_count
                ));
            }
            md.push('\n');
        }
        if !pack.implementation_plan.is_empty() {
            md.push_str("## Likely implementation plan\n\n");
            for (i, step) in pack.implementation_plan.iter().enumerate() {
                md.push_str(&format!("{}. {}\n", i + 1, Self::ascii_arrow(step)));
            }
            md.push('\n');
        }
        if !pack.dependency_paths.is_empty() {
            md.push_str("## Dependency paths\n\n");
            for path in &pack.dependency_paths {
                md.push_str(&Self::ascii_arrow(path));
                md.push('\n');
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
        md.push_str("## Suggested files to read before editing\n\n");
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
                Confidence::High => "HIGH",
                Confidence::Medium => "MEDIUM",
                Confidence::Low => "LOW",
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
                md.push_str(&format!("- {}\n", Self::ascii_arrow(rule)));
            }
            md.push('\n');
        }
        md
    }
}

impl ContextPack {
    /// Deduplicate all fields by path, symbol text, warning text, etc.
    pub fn dedupe(&mut self) {
        let mut seen_paths = HashSet::new();
        self.relevant_files.retain(|f| seen_paths.insert(f.path.clone()));

        for file in &mut self.relevant_files {
            let mut seen_syms = HashSet::new();
            file.important_symbols.retain(|s| seen_syms.insert(s.clone()));
        }

        let mut seen_deps = HashSet::new();
        self.dependency_paths.retain(|p| seen_deps.insert(p.clone()));

        let mut seen_rd = HashSet::new();
        self.reverse_dependencies.retain(|rd| seen_rd.insert(rd.path.clone()));
        for rd in &mut self.reverse_dependencies {
            let mut seen_users = HashSet::new();
            rd.used_by.retain(|u| seen_users.insert(u.clone()));
        }

        let mut seen_warnings = HashSet::new();
        self.risk_warnings.retain(|w| seen_warnings.insert(w.clone()));

        let mut seen_read = HashSet::new();
        self.suggested_read_order.retain(|r| seen_read.insert(r.clone()));

        let mut seen_tests = HashSet::new();
        self.suggested_tests.retain(|t| seen_tests.insert(t.clone()));

        let mut seen_rules = HashSet::new();
        self.architecture_rules.retain(|r| seen_rules.insert(r.clone()));

        let mut seen_steps = HashSet::new();
        self.implementation_plan.retain(|s| seen_steps.insert(s.clone()));
    }
}

fn extract_keywords(task: &str) -> Vec<String> {
    let stop_words = [
        "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "have",
        "has", "had", "do", "does", "did", "will", "would", "could", "should", "may",
        "might", "shall", "can", "fix", "add", "change", "update", "remove", "implement",
        "make", "get", "set", "new", "this", "that", "these", "those", "to", "of", "in",
        "for", "on", "with", "at", "by", "from", "as", "into", "about", "after", "before",
        "between", "under", "and", "but", "or", "nor", "not", "so", "yet", "no", "bug",
        "feature", "issue", "task", "when",
    ];
    let mut words: Vec<String> = task
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .filter(|w| !stop_words.contains(w))
        .map(|w| w.to_lowercase())
        .collect();
    words.sort();
    words.dedup();
    words
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").to_lowercase()
}

fn score_file_relevance(path: &str, keywords: &[String], is_backend_task: bool) -> f64 {
    let lower_path = normalize_path(path);
    let mut score = score_relevance(path, path, keywords);

    if lower_path.contains("/src/") {
        score += 1.0;
    }
    if lower_path.contains("benchmarks/fixtures") || lower_path.contains("/fixtures/") {
        score -= 5.0;
    }
    if lower_path.contains("target/") || lower_path.contains("node_modules/") {
        score -= 10.0;
    }

    if is_backend_task {
        if lower_path.ends_with("cargo.toml") || lower_path.ends_with("package.json") {
            score += 3.0;
        }
        if lower_path.ends_with("/lib.rs") || lower_path.ends_with("/mod.rs") {
            score += 2.5;
        }
        if lower_path.contains("/db.rs")
            || lower_path.contains("database")
            || lower_path.contains("storage")
            || lower_path.contains("cache")
        {
            score += 2.0;
        }
    }

    score
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

fn symbol_relevance_score(node: &GraphNode, keywords: &[String]) -> f64 {
    let lower_name = node.name.to_lowercase();
    let lower_sig = node.signature.as_deref().unwrap_or_default().to_lowercase();
    let mut score = 0.0;

    for kw in keywords {
        if lower_name.contains(kw) {
            score += 2.0;
        }
        if lower_sig.contains(kw) {
            score += 1.0;
        }
    }

    if matches!(
        node.kind,
        NodeKind::Struct | NodeKind::Class | NodeKind::Enum | NodeKind::Trait | NodeKind::Interface
    ) {
        score += 2.5;
    }

    if lower_name == "cachedb" {
        score += 10.0;
    }
    if lower_name.contains("open") || lower_name.contains("initialize") {
        score += 6.0;
    }
    if lower_name.contains("config") {
        score += 5.0;
    }
    if lower_name.contains("search_vector") || lower_name.contains("search_document") {
        score += 4.5;
    }
    if lower_name.contains("insert")
        || lower_name.contains("upsert")
        || lower_name.contains("delete")
        || lower_name.contains("clear")
    {
        score += 3.0;
    }
    if lower_name.contains("count") || lower_name.contains("stats") || lower_name.contains("language_stats") {
        score -= 2.0;
    }

    score
}
