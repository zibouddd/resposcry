use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use reposcry_cache::db::CacheDb;
use reposcry_context::{ContextBuilder, ContextConfig, OutputFormat};
use reposcry_git::{GitChange, GitIntegration};
use reposcry_graph::edge::EdgeKind;
use reposcry_graph::graph::CodeGraph;
use reposcry_graph::node::{GraphNode, NodeKind};
use serde::Serialize;
use serde_json::json;

const CACHE_DIR: &str = ".reposcry";
const CACHE_DB: &str = "reposcry.db";

#[derive(Parser)]
#[command(
    name = "reposcry-crg",
    version,
    about = "Code-review-graph compatible commands for RepoScry"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(global = true, long = "repo", short = 'C', default_value = ".")]
    repo_root: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Reviewing code changes — gives risk-scored analysis.
    #[command(name = "detect_changes", visible_alias = "detect-changes")]
    DetectChanges {
        base: String,
        #[arg(default_value = "HEAD")]
        head: String,
        #[arg(long, default_value = "markdown")]
        format: String,
    },
    /// Need source context for review — token-efficient.
    #[command(name = "get_review_context", visible_alias = "get-review-context")]
    GetReviewContext {
        task: String,
        #[arg(long, default_value = "20000")]
        budget: u32,
        #[arg(long)]
        strict: bool,
        #[arg(long, default_value = "markdown")]
        format: String,
    },
    /// Understanding blast radius of a file or symbol.
    #[command(name = "get_impact_radius", visible_alias = "get-impact-radius")]
    GetImpactRadius {
        target: String,
        #[arg(long, default_value = "3")]
        depth: usize,
        #[arg(long, default_value = "markdown")]
        format: String,
    },
    /// Finding which entrypoint-like execution flows are impacted.
    #[command(name = "get_affected_flows", visible_alias = "get-affected-flows")]
    GetAffectedFlows {
        base: String,
        #[arg(default_value = "HEAD")]
        head: String,
        #[arg(long, default_value = "markdown")]
        format: String,
    },
    /// Trace callers, callees, imports, tests, and dependencies.
    #[command(name = "query_graph", visible_alias = "query-graph")]
    QueryGraph {
        query: String,
        #[arg(long, default_value = "json")]
        format: String,
    },
    /// Find functions/classes/files by name, path, signature, or keyword.
    #[command(name = "semantic_search_nodes", visible_alias = "semantic-search-nodes")]
    SemanticSearchNodes {
        query: String,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long, default_value = "20")]
        limit: usize,
        #[arg(long, default_value = "json")]
        format: String,
    },
    /// Understand high-level codebase structure.
    #[command(name = "get_architecture_overview", visible_alias = "get-architecture-overview")]
    GetArchitectureOverview {
        #[arg(long, default_value = "markdown")]
        format: String,
    },
    /// Plan renames, file splits, and dead-code cleanup.
    #[command(name = "refactor_tool", visible_alias = "refactor-tool")]
    RefactorTool {
        #[arg(default_value = "dead-code")]
        action: String,
        target: Option<String>,
        replacement: Option<String>,
        #[arg(long, default_value = "json")]
        format: String,
    },
}

#[derive(Debug, Clone, Serialize)]
struct NodeSummary {
    id: u64,
    name: String,
    kind: String,
    file_path: Option<String>,
    line: Option<u32>,
    signature: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct EdgeSummary {
    source: NodeSummary,
    target: NodeSummary,
    kind: String,
}

#[derive(Debug, Clone, Serialize)]
struct ChangedFileSummary {
    path: String,
    status: String,
    lines_added: i64,
    lines_deleted: i64,
    fan_in: usize,
    fan_out: usize,
}

#[derive(Debug, Clone, Serialize)]
struct ImpactedFile {
    path: String,
    depth: usize,
    reason: String,
}

#[derive(Debug, Clone, Serialize)]
struct DetectChangesOutput {
    tool: &'static str,
    base: String,
    head: String,
    risk_score: u32,
    risk_level: String,
    risk_reasons: Vec<String>,
    changed_files: Vec<ChangedFileSummary>,
    impacted_files: Vec<ImpactedFile>,
    affected_flows: Vec<FlowSummary>,
    suggested_tests: Vec<String>,
    review_focus: Vec<String>,
    graph_limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ImpactRadiusOutput {
    tool: &'static str,
    target: String,
    depth: usize,
    start_nodes: Vec<NodeSummary>,
    impacted_files: Vec<ImpactedFile>,
    suggested_tests: Vec<String>,
    graph_limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct FlowSummary {
    entrypoint: String,
    kind: String,
    reason: String,
    touched_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct AffectedFlowsOutput {
    tool: &'static str,
    base: String,
    head: String,
    changed_files: Vec<String>,
    flows: Vec<FlowSummary>,
    graph_limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct QueryGraphOutput {
    tool: &'static str,
    query: String,
    mode: String,
    nodes: Vec<NodeSummary>,
    edges: Vec<EdgeSummary>,
}

#[derive(Debug, Clone, Serialize)]
struct SearchHit {
    score: f64,
    node: NodeSummary,
}

#[derive(Debug, Clone, Serialize)]
struct SemanticSearchOutput {
    tool: &'static str,
    query: String,
    kind: Option<String>,
    hits: Vec<SearchHit>,
}

#[derive(Debug, Clone, Serialize)]
struct FileDegree {
    path: String,
    count: usize,
}

#[derive(Debug, Clone, Serialize)]
struct ModuleSummary {
    module: String,
    files: usize,
    loc: i64,
}

#[derive(Debug, Clone, Serialize)]
struct ArchitectureOverviewOutput {
    tool: &'static str,
    files_indexed: i64,
    symbols_indexed: i64,
    imports_indexed: i64,
    resolved_import_edges: i64,
    languages: Vec<(String, i64)>,
    modules: Vec<ModuleSummary>,
    top_fan_in: Vec<FileDegree>,
    top_fan_out: Vec<FileDegree>,
    cycles: Vec<Vec<String>>,
    graph_limitations: Vec<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo_root = PathBuf::from(&cli.repo_root).canonicalize()?;
    let db_path = repo_root.join(CACHE_DIR).join(CACHE_DB);

    match cli.command {
        Commands::DetectChanges { base, head, format } => {
            cmd_detect_changes(&repo_root, &db_path, &base, &head, &format)
        }
        Commands::GetReviewContext {
            task,
            budget,
            strict,
            format,
        } => cmd_get_review_context(&repo_root, &db_path, &task, budget, strict, &format),
        Commands::GetImpactRadius {
            target,
            depth,
            format,
        } => cmd_get_impact_radius(&db_path, &target, depth, &format),
        Commands::GetAffectedFlows { base, head, format } => {
            cmd_get_affected_flows(&repo_root, &db_path, &base, &head, &format)
        }
        Commands::QueryGraph { query, format } => cmd_query_graph(&db_path, &query, &format),
        Commands::SemanticSearchNodes {
            query,
            kind,
            limit,
            format,
        } => cmd_semantic_search_nodes(&db_path, &query, kind.as_deref(), limit, &format),
        Commands::GetArchitectureOverview { format } => {
            cmd_get_architecture_overview(&db_path, &format)
        }
        Commands::RefactorTool {
            action,
            target,
            replacement,
            format,
        } => cmd_refactor_tool(
            &db_path,
            &action,
            target.as_deref(),
            replacement.as_deref(),
            &format,
        ),
    }
}

fn cmd_detect_changes(
    repo_root: &Path,
    db_path: &Path,
    base: &str,
    head: &str,
    format: &str,
) -> Result<()> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let git = GitIntegration::new(repo_root);
    let changes = git.diff_files(base, head)?;
    let changed_paths: Vec<String> = changes.iter().map(|change| change.path.clone()).collect();
    let impacted_files = walk_reverse_imports(&graph, &changed_paths, 3);
    let suggested_tests = suggested_tests(&graph, &changed_paths, 20);
    let flows = infer_flows(&graph, &changed_paths, &impacted_files);
    let risk_reasons = risk_reasons(&graph, &changes, &impacted_files, &flows, &suggested_tests);
    let risk_score = risk_score(&changes, &impacted_files, &flows, &risk_reasons);
    let changed_files = changes
        .iter()
        .map(|change| ChangedFileSummary {
            path: change.path.clone(),
            status: change.status.clone(),
            lines_added: change.lines_added,
            lines_deleted: change.lines_deleted,
            fan_in: fan_in(&graph, &change.path),
            fan_out: fan_out(&graph, &change.path),
        })
        .collect::<Vec<_>>();
    let review_focus = review_focus(&risk_reasons, &flows, &suggested_tests);
    let output = DetectChangesOutput {
        tool: "detect_changes",
        base: base.to_string(),
        head: head.to_string(),
        risk_score,
        risk_level: risk_level(risk_score).to_string(),
        risk_reasons,
        changed_files,
        impacted_files,
        affected_flows: flows,
        suggested_tests,
        review_focus,
        graph_limitations: default_graph_limitations(),
    };
    if is_json(format) {
        print_json(&output)
    } else {
        print_detect_changes_markdown(&output);
        Ok(())
    }
}

fn cmd_get_review_context(
    repo_root: &Path,
    db_path: &Path,
    task: &str,
    budget: u32,
    strict: bool,
    format: &str,
) -> Result<()> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let git = GitIntegration::new(repo_root);
    let config = ContextConfig {
        token_budget: budget,
        strict_mode: strict,
        max_files: 30,
        max_reverse_depth: 2,
        include_full_files: false,
        format: if is_json(format) {
            OutputFormat::Json
        } else {
            OutputFormat::Markdown
        },
    };
    let pack = ContextBuilder::new(graph, config)
        .with_cache(db)
        .with_git(git)
        .build(task)?;
    if is_json(format) {
        print_json(&json!({
            "tool": "get_review_context",
            "context": pack,
            "graph_limitations": default_graph_limitations(),
        }))
    } else {
        let renderer = ContextBuilder::new(CodeGraph::new(), ContextConfig::default());
        println!("{}", renderer.render_markdown(&pack));
        Ok(())
    }
}

fn cmd_get_impact_radius(db_path: &Path, target: &str, depth: usize, format: &str) -> Result<()> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let start_nodes = find_matching_nodes(&graph, target)
        .into_iter()
        .map(NodeSummary::from)
        .collect::<Vec<_>>();
    let start_paths = start_nodes
        .iter()
        .filter_map(|node| node.file_path.clone())
        .collect::<Vec<_>>();
    let impacted_files = walk_reverse_imports(&graph, &start_paths, depth);
    let suggested_tests = suggested_tests(&graph, &start_paths, 20);
    let output = ImpactRadiusOutput {
        tool: "get_impact_radius",
        target: target.to_string(),
        depth,
        start_nodes,
        impacted_files,
        suggested_tests,
        graph_limitations: default_graph_limitations(),
    };
    if is_json(format) {
        print_json(&output)
    } else {
        print_impact_radius_markdown(&output);
        Ok(())
    }
}

fn cmd_get_affected_flows(
    repo_root: &Path,
    db_path: &Path,
    base: &str,
    head: &str,
    format: &str,
) -> Result<()> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let git = GitIntegration::new(repo_root);
    let changes = git.diff_files(base, head)?;
    let changed_files = changes.iter().map(|change| change.path.clone()).collect::<Vec<_>>();
    let impacted_files = walk_reverse_imports(&graph, &changed_files, 4);
    let flows = infer_flows(&graph, &changed_files, &impacted_files);
    let output = AffectedFlowsOutput {
        tool: "get_affected_flows",
        base: base.to_string(),
        head: head.to_string(),
        changed_files,
        flows,
        graph_limitations: default_graph_limitations(),
    };
    if is_json(format) {
        print_json(&output)
    } else {
        print_flows_markdown(&output);
        Ok(())
    }
}

fn cmd_query_graph(db_path: &Path, query: &str, format: &str) -> Result<()> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let query_trimmed = query.trim();
    let query_lower = query_trimmed.to_lowercase();
    let (mode, nodes, edges) = if let Some(selector) = query_lower
        .strip_prefix("imports_of ")
        .or_else(|| query_lower.strip_prefix("deps_of "))
        .or_else(|| query_lower.strip_prefix("callees_of "))
    {
        let original_selector = &query_trimmed[query_trimmed.len() - selector.len()..];
        graph_edges_for_selector(&graph, original_selector, Direction::Outgoing, "imports")
    } else if let Some(selector) = query_lower
        .strip_prefix("imported_by ")
        .or_else(|| query_lower.strip_prefix("rdeps_of "))
        .or_else(|| query_lower.strip_prefix("callers_of "))
    {
        let original_selector = &query_trimmed[query_trimmed.len() - selector.len()..];
        graph_edges_for_selector(&graph, original_selector, Direction::Incoming, "reverse_imports")
    } else if let Some(selector) = query_lower.strip_prefix("symbols_in ") {
        let original_selector = &query_trimmed[query_trimmed.len() - selector.len()..];
        let nodes = graph
            .nodes
            .values()
            .filter(|node| node.kind != NodeKind::File)
            .filter(|node| node.file_path.as_deref() == Some(original_selector))
            .map(NodeSummary::from)
            .collect::<Vec<_>>();
        ("symbols_in".to_string(), nodes, Vec::new())
    } else if let Some(selector) = query_lower.strip_prefix("tests_for ") {
        let original_selector = &query_trimmed[query_trimmed.len() - selector.len()..];
        let tests = suggested_tests(&graph, &[original_selector.to_string()], 50)
            .into_iter()
            .filter_map(|path| file_node_by_path(&graph, &path))
            .map(NodeSummary::from)
            .collect::<Vec<_>>();
        ("tests_for".to_string(), tests, Vec::new())
    } else {
        let nodes = search_nodes(&graph, query_trimmed, None, 50)
            .into_iter()
            .map(|hit| hit.node)
            .collect::<Vec<_>>();
        ("search".to_string(), nodes, Vec::new())
    };
    let output = QueryGraphOutput {
        tool: "query_graph",
        query: query.to_string(),
        mode,
        nodes,
        edges,
    };
    if is_json(format) {
        print_json(&output)
    } else {
        print_query_graph_markdown(&output);
        Ok(())
    }
}

fn cmd_semantic_search_nodes(
    db_path: &Path,
    query: &str,
    kind: Option<&str>,
    limit: usize,
    format: &str,
) -> Result<()> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let output = SemanticSearchOutput {
        tool: "semantic_search_nodes",
        query: query.to_string(),
        kind: kind.map(|value| value.to_string()),
        hits: search_nodes(&graph, query, kind, limit),
    };
    if is_json(format) {
        print_json(&output)
    } else {
        print_search_markdown(&output);
        Ok(())
    }
}

fn cmd_get_architecture_overview(db_path: &Path, format: &str) -> Result<()> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let files = db.get_all_files()?;
    let output = ArchitectureOverviewOutput {
        tool: "get_architecture_overview",
        files_indexed: db.file_count()?,
        symbols_indexed: db.symbol_count()?,
        imports_indexed: db.import_count()?,
        resolved_import_edges: db.edge_count()?,
        languages: db.language_stats()?,
        modules: summarize_modules(&files),
        top_fan_in: top_degrees(&graph, Direction::Incoming, 10),
        top_fan_out: top_degrees(&graph, Direction::Outgoing, 10),
        cycles: graph
            .detect_cycles()
            .into_iter()
            .map(|cycle| {
                cycle
                    .into_iter()
                    .filter_map(|node_id| graph.get_node(node_id))
                    .map(display_node)
                    .collect::<Vec<_>>()
            })
            .collect(),
        graph_limitations: default_graph_limitations(),
    };
    if is_json(format) {
        print_json(&output)
    } else {
        print_architecture_markdown(&output);
        Ok(())
    }
}

fn cmd_refactor_tool(
    db_path: &Path,
    action: &str,
    target: Option<&str>,
    replacement: Option<&str>,
    format: &str,
) -> Result<()> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let action_lower = action.to_lowercase();
    let payload = match action_lower.as_str() {
        "dead-code" | "dead_code" | "deadcode" => {
            let candidates = graph
                .nodes
                .values()
                .filter(|node| node.kind == NodeKind::File)
                .filter(|node| {
                    node.file_path.as_deref().map_or(false, |path| {
                        !is_test_file(path)
                            && !is_config_or_doc(path)
                            && fan_in(&graph, path) == 0
                            && fan_out(&graph, path) == 0
                    })
                })
                .take(100)
                .map(|node| {
                    json!({
                        "path": node.file_path,
                        "confidence": "low",
                        "reason": "No indexed import edges in or out. Dynamic/runtime usage is not ruled out."
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "tool": "refactor_tool",
                "action": "dead-code",
                "candidates": candidates,
                "warnings": ["Treat dead-code output as a candidate list until call/reference edges are indexed."],
            })
        }
        "rename" => {
            let target = target.ok_or_else(|| anyhow!("rename requires a target"))?;
            let matches = find_matching_nodes(&graph, target)
                .into_iter()
                .map(NodeSummary::from)
                .collect::<Vec<_>>();
            let start_paths = matches
                .iter()
                .filter_map(|node| node.file_path.clone())
                .collect::<Vec<_>>();
            json!({
                "tool": "refactor_tool",
                "action": "rename",
                "target": target,
                "replacement": replacement,
                "matches": matches,
                "impacted_files": walk_reverse_imports(&graph, &start_paths, 3),
                "warnings": ["This is a dry-run plan. RepoScry does not rewrite files from this command."],
            })
        }
        "split-file" | "split_file" => {
            let target = target.ok_or_else(|| anyhow!("split-file requires a target file"))?;
            let symbols = graph
                .nodes
                .values()
                .filter(|node| node.kind != NodeKind::File)
                .filter(|node| node.file_path.as_deref() == Some(target))
                .map(NodeSummary::from)
                .collect::<Vec<_>>();
            json!({
                "tool": "refactor_tool",
                "action": "split-file",
                "target": target,
                "symbols": symbols,
                "fan_in": fan_in(&graph, target),
                "fan_out": fan_out(&graph, target),
                "suggested_strategy": [
                    "Group symbols by responsibility and imports.",
                    "Move low-level helpers first.",
                    "Keep public exports stable until callers are migrated."
                ]
            })
        }
        _ => return Err(anyhow!("unknown refactor action: {}", action)),
    };

    if is_json(format) {
        print_json(&payload)
    } else {
        println!("{}", serde_json::to_string_pretty(&payload)?);
        Ok(())
    }
}

fn rebuild_graph(db: &CacheDb) -> Result<CodeGraph> {
    let files = db.get_all_files()?;
    let mut graph = CodeGraph::new();
    let mut db_file_to_graph_node: HashMap<i64, u64> = HashMap::new();

    for file in &files {
        let graph_file_id = graph.add_node(NodeKind::File, &file.path);
        db_file_to_graph_node.insert(file.id, graph_file_id);
        if let Some(node) = graph.nodes.get_mut(&graph_file_id) {
            node.file_path = Some(file.path.clone());
            node.language = Some(file.language.clone());
        }
    }

    for file in &files {
        let Some(&graph_file_id) = db_file_to_graph_node.get(&file.id) else {
            continue;
        };
        for sym in db.get_symbols_by_file(file.id)? {
            let sym_id = graph.add_node(symbol_kind(&sym.kind), &sym.name);
            if let Some(node) = graph.nodes.get_mut(&sym_id) {
                node.file_path = Some(file.path.clone());
                node.start_line = Some(sym.start_line);
                node.end_line = Some(sym.end_line);
                node.signature = sym.signature.clone();
                node.visibility = sym.visibility.clone();
                node.doc_comment = sym.doc_comment.clone();
            }
            graph.add_edge(graph_file_id, sym_id, EdgeKind::Contains);
        }
    }

    for cached_edge in db.get_edges_by_kind(EdgeKind::Imports)? {
        let Some(&source_graph_id) = db_file_to_graph_node.get(&cached_edge.source_file_id) else {
            continue;
        };
        let Some(target_file_id) = cached_edge.target_file_id else {
            continue;
        };
        let Some(&target_graph_id) = db_file_to_graph_node.get(&target_file_id) else {
            continue;
        };
        graph.add_edge(source_graph_id, target_graph_id, EdgeKind::Imports);
    }

    Ok(graph)
}

fn symbol_kind(kind: &str) -> NodeKind {
    match kind {
        "function" => NodeKind::Function,
        "method" => NodeKind::Method,
        "struct" => NodeKind::Struct,
        "enum" => NodeKind::Enum,
        "trait" => NodeKind::Trait,
        "class" => NodeKind::Class,
        "interface" | "type" => NodeKind::Interface,
        "component" => NodeKind::Component,
        "test" => NodeKind::Test,
        "hook" => NodeKind::Hook,
        "api_handler" => NodeKind::ApiEndpoint,
        "module" => NodeKind::Module,
        _ => NodeKind::Symbol,
    }
}

impl From<&GraphNode> for NodeSummary {
    fn from(node: &GraphNode) -> Self {
        Self {
            id: node.id,
            name: node.name.clone(),
            kind: node.kind.as_str().to_string(),
            file_path: node.file_path.clone(),
            line: node.start_line,
            signature: node.signature.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Direction {
    Incoming,
    Outgoing,
}

fn graph_edges_for_selector(
    graph: &CodeGraph,
    selector: &str,
    direction: Direction,
    mode: &str,
) -> (String, Vec<NodeSummary>, Vec<EdgeSummary>) {
    let start_nodes = find_matching_nodes(graph, selector);
    let start_ids = start_nodes.iter().map(|node| node.id).collect::<HashSet<_>>();
    let edges = graph
        .edges
        .iter()
        .filter(|edge| edge.kind == EdgeKind::Imports || edge.kind == EdgeKind::Calls)
        .filter(|edge| match direction {
            Direction::Incoming => start_ids.contains(&edge.target_id),
            Direction::Outgoing => start_ids.contains(&edge.source_id),
        })
        .filter_map(|edge| {
            Some(EdgeSummary {
                source: NodeSummary::from(graph.get_node(edge.source_id)?),
                target: NodeSummary::from(graph.get_node(edge.target_id)?),
                kind: edge.kind.as_str().to_string(),
            })
        })
        .collect::<Vec<_>>();
    (
        mode.to_string(),
        start_nodes.into_iter().map(NodeSummary::from).collect(),
        edges,
    )
}

fn find_matching_nodes<'a>(graph: &'a CodeGraph, selector: &str) -> Vec<&'a GraphNode> {
    let selector_lower = selector.to_lowercase();
    let mut nodes = graph
        .nodes
        .values()
        .filter(|node| {
            node.name.to_lowercase().contains(&selector_lower)
                || node
                    .file_path
                    .as_deref()
                    .map_or(false, |path| path == selector || path.to_lowercase().contains(&selector_lower))
        })
        .collect::<Vec<_>>();
    nodes.sort_by_key(|node| (node.file_path.clone().unwrap_or_default(), node.start_line.unwrap_or(0)));
    nodes
}

fn search_nodes(graph: &CodeGraph, query: &str, kind: Option<&str>, limit: usize) -> Vec<SearchHit> {
    let kind = kind.map(|value| value.to_lowercase());
    let terms = search_terms(query);
    let query_lower = query.to_lowercase();
    let mut hits = graph
        .nodes
        .values()
        .filter(|node| {
            kind.as_deref()
                .map_or(true, |wanted| node.kind.as_str() == wanted)
        })
        .filter_map(|node| {
            let haystack = format!(
                "{} {} {} {}",
                node.name,
                node.kind.as_str(),
                node.file_path.as_deref().unwrap_or(""),
                node.signature.as_deref().unwrap_or("")
            )
            .to_lowercase();
            let mut score = 0.0;
            if haystack.contains(&query_lower) {
                score += 5.0;
            }
            for term in &terms {
                if node.name.to_lowercase().contains(term) {
                    score += 3.0;
                }
                if node.file_path.as_deref().unwrap_or("").to_lowercase().contains(term) {
                    score += 2.0;
                }
                if node.signature.as_deref().unwrap_or("").to_lowercase().contains(term) {
                    score += 1.0;
                }
            }
            (score > 0.0).then(|| SearchHit {
                score,
                node: NodeSummary::from(node),
            })
        })
        .collect::<Vec<_>>();
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.node.file_path.cmp(&b.node.file_path))
    });
    hits.truncate(limit);
    hits
}

fn walk_reverse_imports(graph: &CodeGraph, start_paths: &[String], max_depth: usize) -> Vec<ImpactedFile> {
    let mut queue = VecDeque::new();
    let mut visited: HashMap<u64, usize> = HashMap::new();
    let start_path_set = start_paths.iter().cloned().collect::<HashSet<_>>();

    for path in start_paths {
        if let Some(node) = file_node_by_path(graph, path) {
            visited.insert(node.id, 0);
            queue.push_back((node.id, 0));
        }
    }

    while let Some((node_id, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        for edge in graph
            .edges
            .iter()
            .filter(|edge| edge.kind == EdgeKind::Imports && edge.target_id == node_id)
        {
            if let std::collections::hash_map::Entry::Vacant(entry) = visited.entry(edge.source_id) {
                entry.insert(depth + 1);
                queue.push_back((edge.source_id, depth + 1));
            }
        }
    }

    let mut impacted = visited
        .into_iter()
        .filter_map(|(node_id, depth)| {
            let node = graph.get_node(node_id)?;
            let path = node.file_path.clone()?;
            Some(ImpactedFile {
                reason: if start_path_set.contains(&path) {
                    "changed_or_target_file".to_string()
                } else {
                    "reverse_import_dependency".to_string()
                },
                path,
                depth,
            })
        })
        .collect::<Vec<_>>();
    impacted.sort_by(|a, b| a.depth.cmp(&b.depth).then_with(|| a.path.cmp(&b.path)));
    impacted
}

fn infer_flows(graph: &CodeGraph, changed_paths: &[String], impacted_files: &[ImpactedFile]) -> Vec<FlowSummary> {
    let affected_paths = impacted_files
        .iter()
        .map(|file| file.path.clone())
        .chain(changed_paths.iter().cloned())
        .collect::<HashSet<_>>();
    let mut flows = affected_paths
        .iter()
        .filter_map(|path| {
            let kind = classify_flow(path)?;
            Some(FlowSummary {
                entrypoint: path.clone(),
                kind,
                reason: if changed_paths.contains(path) {
                    "entrypoint changed directly".to_string()
                } else {
                    "entrypoint imports a changed file through the indexed graph".to_string()
                },
                touched_files: changed_paths.to_vec(),
            })
        })
        .collect::<Vec<_>>();

    if flows.is_empty() {
        for node in graph.nodes.values().filter(|node| node.kind == NodeKind::File) {
            if let Some(path) = &node.file_path {
                if let Some(kind) = classify_flow(path) {
                    let imports_changed = graph
                        .edges
                        .iter()
                        .filter(|edge| edge.kind == EdgeKind::Imports && edge.source_id == node.id)
                        .filter_map(|edge| graph.get_node(edge.target_id))
                        .filter_map(|target| target.file_path.as_ref())
                        .any(|target_path| changed_paths.contains(target_path));
                    if imports_changed {
                        flows.push(FlowSummary {
                            entrypoint: path.clone(),
                            kind,
                            reason: "entrypoint directly imports a changed file".to_string(),
                            touched_files: changed_paths.to_vec(),
                        });
                    }
                }
            }
        }
    }

    flows.sort_by(|a, b| a.entrypoint.cmp(&b.entrypoint));
    flows.dedup_by(|a, b| a.entrypoint == b.entrypoint);
    flows
}

fn classify_flow(path: &str) -> Option<String> {
    let lower = path.to_lowercase();
    if lower.ends_with("/route.ts")
        || lower.ends_with("/route.tsx")
        || lower.ends_with("/route.js")
        || lower.contains("/pages/api/")
    {
        Some("nextjs_api_route".to_string())
    } else if lower.ends_with("/page.tsx") || lower.ends_with("/page.ts") || lower.ends_with("/page.jsx") {
        Some("nextjs_page".to_string())
    } else if lower == "src/main.rs" || lower.ends_with("/src/main.rs") || lower.contains("/src/bin/") {
        Some("rust_cli_or_binary".to_string())
    } else if lower.contains("worker") || lower.contains("consumer") || lower.contains("queue") {
        Some("worker_or_consumer".to_string())
    } else if is_test_file(path) {
        Some("test_flow".to_string())
    } else {
        None
    }
}

fn risk_reasons(
    graph: &CodeGraph,
    changes: &[GitChange],
    impacted_files: &[ImpactedFile],
    flows: &[FlowSummary],
    suggested_tests: &[String],
) -> Vec<String> {
    let mut reasons = Vec::new();
    if impacted_files.len() > 20 {
        reasons.push(format!("Wide blast radius: {} indexed files are affected.", impacted_files.len()));
    }
    if !flows.is_empty() {
        reasons.push(format!("{} entrypoint-like flow(s) are affected.", flows.len()));
    }
    if suggested_tests.is_empty() && changes.iter().any(|change| !is_test_file(&change.path)) {
        reasons.push("Source files changed but no indexed test candidates were found.".to_string());
    }
    for change in changes {
        let fan_in_count = fan_in(graph, &change.path);
        if fan_in_count > 5 {
            reasons.push(format!("{} has high fan-in: {} dependents.", change.path, fan_in_count));
        }
        if is_risky_path(&change.path) {
            reasons.push(format!("{} touches a high-risk path or concern.", change.path));
        }
    }
    reasons.sort();
    reasons.dedup();
    reasons
}

fn risk_score(
    changes: &[GitChange],
    impacted_files: &[ImpactedFile],
    flows: &[FlowSummary],
    reasons: &[String],
) -> u32 {
    let changed_lines: i64 = changes.iter().map(|change| change.lines_added + change.lines_deleted).sum();
    let mut score = 0u32;
    score += (changes.len() as u32).saturating_mul(5).min(25);
    score += ((changed_lines / 40).max(0) as u32).min(20);
    score += (impacted_files.len() as u32).saturating_mul(2).min(25);
    score += (flows.len() as u32).saturating_mul(8).min(20);
    score += (reasons.len() as u32).saturating_mul(5).min(20);
    score.min(100)
}

fn risk_level(score: u32) -> &'static str {
    match score {
        0..=24 => "low",
        25..=59 => "medium",
        60..=84 => "high",
        _ => "critical",
    }
}

fn review_focus(reasons: &[String], flows: &[FlowSummary], suggested_tests: &[String]) -> Vec<String> {
    let mut focus = Vec::new();
    focus.extend(reasons.iter().take(5).cloned());
    if !flows.is_empty() {
        focus.push("Review affected entrypoints before lower-level helpers.".to_string());
    }
    if !suggested_tests.is_empty() {
        focus.push("Run or update the suggested tests before merging.".to_string());
    }
    if focus.is_empty() {
        focus.push("Review changed files and direct reverse dependencies.".to_string());
    }
    focus
}

fn suggested_tests(graph: &CodeGraph, paths: &[String], limit: usize) -> Vec<String> {
    let terms = paths
        .iter()
        .flat_map(|path| search_terms(path))
        .filter(|term| term.len() > 3)
        .collect::<HashSet<_>>();
    let mut scored = graph
        .nodes
        .values()
        .filter_map(|node| node.file_path.as_ref())
        .filter(|path| is_test_file(path))
        .map(|path| {
            let lower = path.to_lowercase();
            let score = terms.iter().filter(|term| lower.contains(term.as_str())).count();
            (score, path.clone())
        })
        .collect::<Vec<_>>();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    scored.dedup_by(|a, b| a.1 == b.1);
    scored.into_iter().take(limit).map(|(_, path)| path).collect()
}

fn top_degrees(graph: &CodeGraph, direction: Direction, limit: usize) -> Vec<FileDegree> {
    let mut degrees = graph
        .nodes
        .values()
        .filter(|node| node.kind == NodeKind::File)
        .filter_map(|node| {
            let path = node.file_path.clone()?;
            let count = match direction {
                Direction::Incoming => fan_in(graph, &path),
                Direction::Outgoing => fan_out(graph, &path),
            };
            (count > 0).then_some(FileDegree { path, count })
        })
        .collect::<Vec<_>>();
    degrees.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.path.cmp(&b.path)));
    degrees.truncate(limit);
    degrees
}

fn summarize_modules(files: &[reposcry_cache::db::CachedFile]) -> Vec<ModuleSummary> {
    let mut modules: BTreeMap<String, (usize, i64)> = BTreeMap::new();
    for file in files {
        let module = module_name(&file.path);
        let entry = modules.entry(module).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += file.loc;
    }
    let mut output = modules
        .into_iter()
        .map(|(module, (files, loc))| ModuleSummary { module, files, loc })
        .collect::<Vec<_>>();
    output.sort_by(|a, b| b.files.cmp(&a.files).then_with(|| a.module.cmp(&b.module)));
    output
}

fn module_name(path: &str) -> String {
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.len() >= 3 && parts[0] == "crates" {
        format!("crates/{}", parts[1])
    } else {
        parts.first().copied().unwrap_or("root").to_string()
    }
}

fn fan_in(graph: &CodeGraph, path: &str) -> usize {
    file_node_by_path(graph, path)
        .map(|node| {
            graph
                .edges
                .iter()
                .filter(|edge| edge.kind == EdgeKind::Imports && edge.target_id == node.id)
                .count()
        })
        .unwrap_or(0)
}

fn fan_out(graph: &CodeGraph, path: &str) -> usize {
    file_node_by_path(graph, path)
        .map(|node| {
            graph
                .edges
                .iter()
                .filter(|edge| edge.kind == EdgeKind::Imports && edge.source_id == node.id)
                .count()
        })
        .unwrap_or(0)
}

fn file_node_by_path<'a>(graph: &'a CodeGraph, path: &str) -> Option<&'a GraphNode> {
    graph
        .nodes
        .values()
        .find(|node| node.kind == NodeKind::File && node.file_path.as_deref() == Some(path))
}

fn search_terms(input: &str) -> Vec<String> {
    input
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|part| part.len() > 2)
        .map(|part| part.to_lowercase())
        .collect()
}

fn display_node(node: &GraphNode) -> String {
    node.file_path
        .as_ref()
        .map(|path| format!("{} ({})", path, node.kind.as_str()))
        .unwrap_or_else(|| format!("{} ({})", node.name, node.kind.as_str()))
}

fn is_test_file(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains("test") || lower.contains("spec") || lower.contains("__tests__")
}

fn is_config_or_doc(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".md")
        || lower.ends_with(".json")
        || lower.ends_with(".toml")
        || lower.ends_with(".yaml")
        || lower.ends_with(".yml")
        || lower.contains("config")
}

fn is_risky_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    [
        "auth", "login", "session", "payment", "stripe", "billing", "db", "database", "schema",
        "migration", "security", "permission", "route", "api", "env", "config",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn default_graph_limitations() -> Vec<String> {
    vec![
        "Current compatibility commands use indexed imports and symbols; precise call-flow output requires call edges to be indexed.".to_string(),
        "Dynamic imports, reflection, framework magic, and runtime config usage may be under-approximated.".to_string(),
    ]
}

fn is_json(format: &str) -> bool {
    format.eq_ignore_ascii_case("json")
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn print_detect_changes_markdown(output: &DetectChangesOutput) {
    println!("# Change risk analysis\n");
    println!("Base: `{}`  Head: `{}`", output.base, output.head);
    println!("Risk: **{}** ({}/100)\n", output.risk_level, output.risk_score);
    if !output.risk_reasons.is_empty() {
        println!("## Risk reasons");
        for reason in &output.risk_reasons {
            println!("- {}", reason);
        }
        println!();
    }
    println!("## Changed files");
    for file in &output.changed_files {
        println!(
            "- `{}` {} (+{} -{}, fan-in {}, fan-out {})",
            file.path, file.status, file.lines_added, file.lines_deleted, file.fan_in, file.fan_out
        );
    }
    println!();
    print_impacted_files(&output.impacted_files);
    print_flows(&output.affected_flows);
    print_tests(&output.suggested_tests);
}

fn print_impact_radius_markdown(output: &ImpactRadiusOutput) {
    println!("# Impact radius\n");
    println!("Target: `{}`  Depth: `{}`\n", output.target, output.depth);
    println!("## Start nodes");
    for node in &output.start_nodes {
        println!("- `{}` {} {:?}", node.name, node.kind, node.file_path);
    }
    println!();
    print_impacted_files(&output.impacted_files);
    print_tests(&output.suggested_tests);
}

fn print_flows_markdown(output: &AffectedFlowsOutput) {
    println!("# Affected flows\n");
    println!("Base: `{}`  Head: `{}`\n", output.base, output.head);
    print_flows(&output.flows);
}

fn print_query_graph_markdown(output: &QueryGraphOutput) {
    println!("# Graph query\n");
    println!("Query: `{}`  Mode: `{}`\n", output.query, output.mode);
    if !output.nodes.is_empty() {
        println!("## Nodes");
        for node in &output.nodes {
            println!("- `{}` {} {:?}", node.name, node.kind, node.file_path);
        }
        println!();
    }
    if !output.edges.is_empty() {
        println!("## Edges");
        for edge in &output.edges {
            println!("- `{}` --{}--> `{}`", edge.source.name, edge.kind, edge.target.name);
        }
    }
}

fn print_search_markdown(output: &SemanticSearchOutput) {
    println!("# Semantic node search\n");
    println!("Query: `{}`\n", output.query);
    for hit in &output.hits {
        println!(
            "- {:.1} `{}` {} {:?}",
            hit.score, hit.node.name, hit.node.kind, hit.node.file_path
        );
    }
}

fn print_architecture_markdown(output: &ArchitectureOverviewOutput) {
    println!("# Architecture overview\n");
    println!("Files indexed: {}", output.files_indexed);
    println!("Symbols indexed: {}", output.symbols_indexed);
    println!("Imports indexed: {}", output.imports_indexed);
    println!("Resolved import edges: {}\n", output.resolved_import_edges);
    if !output.languages.is_empty() {
        println!("## Languages");
        for (language, count) in &output.languages {
            println!("- {}: {}", language, count);
        }
        println!();
    }
    if !output.modules.is_empty() {
        println!("## Modules");
        for module in output.modules.iter().take(20) {
            println!("- `{}`: {} files, {} LOC", module.module, module.files, module.loc);
        }
        println!();
    }
    println!("## Top fan-in");
    for item in &output.top_fan_in {
        println!("- `{}`: {}", item.path, item.count);
    }
    println!("\n## Top fan-out");
    for item in &output.top_fan_out {
        println!("- `{}`: {}", item.path, item.count);
    }
    if !output.cycles.is_empty() {
        println!("\n## Cycles");
        for cycle in &output.cycles {
            println!("- {}", cycle.join(" -> "));
        }
    }
}

fn print_impacted_files(files: &[ImpactedFile]) {
    if files.is_empty() {
        println!("## Impacted files\nNo indexed impacted files found.\n");
        return;
    }
    println!("## Impacted files");
    for file in files {
        println!("- depth {} `{}` — {}", file.depth, file.path, file.reason);
    }
    println!();
}

fn print_flows(flows: &[FlowSummary]) {
    if flows.is_empty() {
        println!("## Affected flows\nNo entrypoint-like flows found in the indexed graph.\n");
        return;
    }
    println!("## Affected flows");
    for flow in flows {
        println!("- `{}` ({}) — {}", flow.entrypoint, flow.kind, flow.reason);
    }
    println!();
}

fn print_tests(tests: &[String]) {
    if tests.is_empty() {
        return;
    }
    println!("## Suggested tests");
    for test in tests {
        println!("- `{}`", test);
    }
    println!();
}
