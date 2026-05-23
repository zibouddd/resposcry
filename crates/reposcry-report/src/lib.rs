use anyhow::Result;
use serde::{Deserialize, Serialize};

use reposcry_context::ContextPack;
use reposcry_git::{GitChange, GitIntegration};
use reposcry_graph::edge::EdgeKind;
use reposcry_graph::graph::CodeGraph;
use reposcry_rules::{RuleViolation, RulesEngine};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewReport {
    pub summary: ReportSummary,
    pub changed_files: Vec<GitChange>,
    pub high_risk_changes: Vec<RiskItem>,
    pub new_dependencies: Vec<String>,
    pub new_cycles: Vec<Vec<String>>,
    pub violations: Vec<RuleViolation>,
    pub suggested_reviewers: Vec<String>,
    pub suggested_tests: Vec<String>,
    pub context: Option<ContextPack>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSummary {
    pub changed_files_count: u32,
    pub impacted_files_count: u32,
    pub risk: RiskLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskItem {
    pub file: String,
    pub reason: String,
}

pub fn generate_report(
    graph: &CodeGraph,
    git: &GitIntegration,
    rules_engine: &RulesEngine,
    base: &str,
    head: &str,
) -> Result<ReviewReport> {
    // Get changed files
    let changes = git.diff_files(base, head)?;
    let changed_paths: Vec<String> = changes.iter().map(|c| c.path.clone()).collect();

    // Find impacted files (reverse deps of changed files)
    let mut impacted = Vec::new();
    for edge in graph
        .edges
        .iter()
        .filter(|edge| edge.kind == EdgeKind::Imports)
    {
        let source_path = graph
            .nodes
            .get(&edge.source_id)
            .and_then(|n| n.file_path.as_deref());
        let target_path = graph
            .nodes
            .get(&edge.target_id)
            .and_then(|n| n.file_path.as_deref());
        if let Some(tp) = target_path {
            if changed_paths.contains(&tp.to_string()) {
                if let Some(sp) = source_path {
                    if !changed_paths.contains(&sp.to_string()) {
                        impacted.push(sp.to_string());
                    }
                }
            }
        }
    }
    impacted.sort();
    impacted.dedup();

    // Compute risk
    let risk = if impacted.len() > 20 {
        RiskLevel::High
    } else if impacted.len() > 5 {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    };

    // High risk items
    let mut high_risk = Vec::new();
    for path in &changed_paths {
        let rdeps = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Imports)
            .filter(|e| {
                graph
                    .nodes
                    .get(&e.target_id)
                    .and_then(|n| n.file_path.as_deref())
                    == Some(path.as_str())
            })
            .count();
        if rdeps > 5 {
            high_risk.push(RiskItem {
                file: path.clone(),
                reason: format!("high fan-in ({} dependents), changed", rdeps),
            });
        }
    }

    // Detect cycles
    let cycles = graph.detect_cycles();
    let new_cycles: Vec<Vec<String>> = cycles
        .into_iter()
        .map(|cycle| {
            cycle
                .iter()
                .filter_map(|id| graph.get_node(*id))
                .map(|n| n.name.clone())
                .collect()
        })
        .collect();

    // Check rules
    let violations = rules_engine.check_graph(graph);

    // Suggested reviewers
    let mut suggested_reviewers = Vec::new();
    for path in &changed_paths {
        if let Ok(owner) = git.file_owner(path) {
            if !suggested_reviewers.contains(&owner) {
                suggested_reviewers.push(owner);
            }
        }
    }

    Ok(ReviewReport {
        summary: ReportSummary {
            changed_files_count: changes.len() as u32,
            impacted_files_count: impacted.len() as u32,
            risk,
        },
        changed_files: changes,
        high_risk_changes: high_risk,
        new_dependencies: Vec::new(),
        new_cycles,
        violations,
        suggested_reviewers,
        suggested_tests: Vec::new(),
        context: None,
    })
}

pub fn render_markdown(report: &ReviewReport) -> String {
    let mut md = String::new();
    md.push_str("# RepoScry Report\n\n");
    md.push_str(&format!(
        "## Summary\n\nChanged files: {}\nImpacted files: {}\nRisk: {:?}\n\n",
        report.summary.changed_files_count,
        report.summary.impacted_files_count,
        report.summary.risk
    ));
    if !report.high_risk_changes.is_empty() {
        md.push_str("## High-risk changes\n\n| File | Reason |\n|---|---|\n");
        for item in &report.high_risk_changes {
            md.push_str(&format!("| {} | {} |\n", item.file, item.reason));
        }
        md.push('\n');
    }
    if !report.new_cycles.is_empty() {
        md.push_str("## New dependency cycles\n\n");
        for cycle in &report.new_cycles {
            md.push_str(&format!("- {}\n", cycle.join(" → ")));
        }
        md.push('\n');
    }
    if !report.violations.is_empty() {
        md.push_str("## Architecture violations\n\n");
        for v in &report.violations {
            md.push_str(&format!(
                "- [{}] {}: {}\n",
                v.severity.as_str(),
                v.rule,
                v.message
            ));
        }
        md.push('\n');
    }
    if !report.suggested_reviewers.is_empty() {
        md.push_str("## Suggested reviewers\n\n");
        for reviewer in &report.suggested_reviewers {
            md.push_str(&format!("- {}\n", reviewer));
        }
        md.push('\n');
    }
    md
}
