use std::path::Path;

use anyhow::Result;
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use reposcry_graph::edge::EdgeKind;
use reposcry_graph::graph::CodeGraph;
use reposcry_indexer::scanner::ScannedFile;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub name: String,
    pub description: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub path: Option<String>,
    pub max_lines: Option<u32>,
    pub severity: Severity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleViolation {
    pub rule: String,
    pub severity: Severity,
    pub message: String,
    pub source_path: Option<String>,
    pub target_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulesConfig {
    pub rules: Vec<Rule>,
}

impl RulesConfig {
    pub fn default_rules() -> Self {
        Self {
            rules: vec![
                Rule {
                    name: "no-ui-to-db".into(),
                    description: Some("UI must not import database code directly.".into()),
                    from: Some("src/components/**".into()),
                    to: Some("src/server/db/**".into()),
                    path: None,
                    max_lines: None,
                    severity: Severity::Error,
                },
                Rule {
                    name: "no-api-to-ui".into(),
                    description: Some("API routes must not import UI components.".into()),
                    from: Some("src/app/api/**".into()),
                    to: Some("src/components/**".into()),
                    path: None,
                    max_lines: None,
                    severity: Severity::Error,
                },
                Rule {
                    name: "no-large-files".into(),
                    description: Some("Files should be under 800 lines.".into()),
                    from: None,
                    to: None,
                    path: None,
                    max_lines: Some(800),
                    severity: Severity::Warning,
                },
                Rule {
                    name: "no-cycles".into(),
                    description: Some("No dependency cycles allowed.".into()),
                    from: None,
                    to: None,
                    path: None,
                    max_lines: None,
                    severity: Severity::Error,
                },
                Rule {
                    name: "no-build-artifacts".into(),
                    description: Some("Build artifacts should not be committed.".into()),
                    from: None,
                    to: None,
                    path: Some("target/**".into()),
                    max_lines: None,
                    severity: Severity::Error,
                },
            ],
        }
    }

    pub fn from_yaml(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(serde_yaml::from_str(&content)?)
    }
}

pub struct RulesEngine {
    config: RulesConfig,
}

impl RulesEngine {
    pub fn new(config: RulesConfig) -> Self {
        Self { config }
    }

    pub fn check_graph(&self, graph: &CodeGraph) -> Vec<RuleViolation> {
        let mut violations = Vec::new();
        for rule in &self.config.rules {
            debug!("Checking rule: {}", rule.name);
            match rule.name.as_str() {
                "no-cycles" => {
                    let cycles = graph.detect_cycles();
                    if !cycles.is_empty() {
                        for cycle in &cycles {
                            let node_names: Vec<String> = cycle
                                .iter()
                                .filter_map(|id| graph.get_node(*id))
                                .map(|n| n.name.clone())
                                .collect();
                            violations.push(RuleViolation {
                                rule: rule.name.clone(),
                                severity: rule.severity.clone(),
                                message: format!(
                                    "Dependency cycle detected: {}",
                                    node_names.join(" → ")
                                ),
                                source_path: None,
                                target_path: None,
                            });
                        }
                    }
                }
                _ => {
                    // Pattern-based rules checked during import resolution
                    if let (Some(from_pattern), Some(to_pattern)) = (&rule.from, &rule.to) {
                        let from_set = build_globset(from_pattern);
                        let to_set = build_globset(to_pattern);
                        if let (Some(from_set), Some(to_set)) = (from_set, to_set) {
                            for edge in &graph.edges {
                                if edge.kind != EdgeKind::Imports {
                                    continue;
                                }
                                let source = graph.get_node(edge.source_id);
                                let target = graph.get_node(edge.target_id);
                                if let (Some(src), Some(tgt)) = (source, target) {
                                    let src_path = src.file_path.as_deref().unwrap_or("");
                                    let tgt_path = tgt.file_path.as_deref().unwrap_or("");
                                    if from_set.is_match(src_path) && to_set.is_match(tgt_path) {
                                        violations.push(RuleViolation {
                                            rule: rule.name.clone(),
                                            severity: rule.severity.clone(),
                                            message: format!(
                                                "{} imports {} (violates {} rule)",
                                                src_path, tgt_path, rule.name
                                            ),
                                            source_path: Some(src_path.to_string()),
                                            target_path: Some(tgt_path.to_string()),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        violations
    }

    pub fn check_files(&self, files: &[ScannedFile]) -> Vec<RuleViolation> {
        let mut violations = Vec::new();
        for rule in &self.config.rules {
            if let Some(max_lines) = rule.max_lines {
                for file in files {
                    if file.size_bytes > 0 {
                        let estimated_lines = file.size_bytes / 40; // rough estimate
                        if estimated_lines > max_lines as u64 {
                            violations.push(RuleViolation {
                                rule: rule.name.clone(),
                                severity: rule.severity.clone(),
                                message: format!(
                                    "{} has ~{} lines (max: {})",
                                    file.relative_path, estimated_lines, max_lines
                                ),
                                source_path: Some(file.relative_path.clone()),
                                target_path: None,
                            });
                        }
                    }
                }
            }
            if let Some(path_pattern) = &rule.path {
                let set = build_globset(path_pattern);
                if let Some(set) = set {
                    for file in files {
                        if set.is_match(&file.relative_path) {
                            violations.push(RuleViolation {
                                rule: rule.name.clone(),
                                severity: rule.severity.clone(),
                                message: format!(
                                    "{} matches excluded path pattern: {}",
                                    file.relative_path, path_pattern
                                ),
                                source_path: Some(file.relative_path.clone()),
                                target_path: None,
                            });
                        }
                    }
                }
            }
        }
        violations
    }
}

fn build_globset(pattern: &str) -> Option<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    match Glob::new(pattern) {
        Ok(glob) => {
            builder.add(glob);
            match builder.build() {
                Ok(set) => Some(set),
                Err(e) => {
                    warn!("Failed to build glob set for '{}': {}", pattern, e);
                    None
                }
            }
        }
        Err(e) => {
            warn!("Invalid glob pattern '{}': {}", pattern, e);
            None
        }
    }
}
