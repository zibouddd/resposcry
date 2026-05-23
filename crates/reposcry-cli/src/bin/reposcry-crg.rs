use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use reposcry_cache::db::{CacheDb, CachedFile};
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
#[command(name = "reposcry-crg", version, about = "CRG-compatible analysis commands for RepoScry")]
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
        #[arg(long, default_value = "json")]
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
        #[arg(long, default_value = "json")]
        format: String,
    },
    /// Understanding blast radius of a file or symbol.
    #[command(name = "get_impact_radius", visible_alias = "get-impact-radius")]
    GetImpactRadius {
        target: String,
        #[arg(long, default_value = "3")]
        depth: usize,
        #[arg(long, default_value = "json")]
        format: String,
    },
    /// Finding which entrypoint-like execution paths are impacted.
    #[command(name = "get_affected_flows", visible_alias = "get-affected-flows")]
    GetAffectedFlows {
        base: String,
        #[arg(default_value = "HEAD")]
        head: String,
        #[arg(long, default_value = "json")]
        format: String,
    },
    /// Tracing callers, callees, imports, tests, and dependencies.
    #[command(name = "query_graph", visible_alias = "query-graph")]
    QueryGraph {
        query: String,
        #[arg(long, default_value = "json")]
        format: String,
    },
    /// Finding functions/classes/files by name, path, signature, or keyword.
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
    /// Understanding high-level codebase structure.
    #[command(name = "get_architecture_overview", visible_alias = "get-architecture-overview")]
    GetArchitectureOverview {
        #[arg(long, default_value = "json")]
        format: String,
    },
    /// Planning renames, file splits, and dead-code cleanup.
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
struct FlowSummary {
    entrypoint: String,
    kind: String,
    reason: String,
    touched_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct SearchHit {
    score: f64,
    node: NodeSummary,
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

#[derive(Debug, Clone, Copy)]
enum Direction {
    Incoming,
    Outgoing,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo_root = PathBuf::from(&cli.repo_root).canonicalize()?;
    let db_path = repo_root.join(CACHE_DIR).join(CACHE_DB);

    match cli.command {
        Commands::DetectChanges { base, head, format } => {
            detect_changes(&repo_root, &db_path, &base, &head, &format)
        }
        Commands::GetReviewContext { task, budget, strict, format } => {
            review_context(&repo_root, &db_path, &task, budget, strict, &format)
        }
        Commands::GetImpactRadius { target, depth, format } => {
            impact_radius(&db_path, &target, depth, &format)
        }
        Commands::GetAffectedFlows { base, head, format } => {
            affected_flows(&repo_root, &db_path, &base, &head, &format)
        }
        Commands::QueryGraph { query, format } => query_graph(&db_path, &query, &format),
        Commands::SemanticSearchNodes { query, kind, limit, format } => {
            semantic_search(&db_path, &query, kind.as_deref(), limit, &format)
        }
        Commands::GetArchitectureOverview { format } => architecture_overview(&db_path, &format),
        Commands::RefactorTool { action, target, replacement, format } => {
            refactor_tool(&db_path, &action, target.as_deref(), replacement.as_deref(), &format)
        }
    }
}

fn detect_changes(repo_root: &Path, db_path: &Path, base: &str, head: &str, format: &str) -> Result<()> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let git = GitIntegration::new(repo_root);
    let changes = git.diff_files(base, head)?;
    let changed_paths = changes.iter().map(|c| c.path.clone()).collect::<Vec<_>>();
    let impacted = walk_reverse_imports(&graph, &changed_paths, 3);
    let flows = infer_flows(&graph, &changed_paths, &impacted);
    let tests = suggested_tests(&graph, &changed_paths, 20);
    let reasons = risk_reasons(&graph, &changes, &impacted, &flows, &tests);
    let score = risk_score(&changes, &impacted, &flows, &reasons);
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

    let output = json!({
        "tool": "detect_changes",
        "base": base,
        "head": head,
        "risk_score": score,
        "risk_level": risk_level(score),
        "risk_reasons": reasons,
        "changed_files": changed_files,
        "impacted_files": impacted,
        "affected_flows": flows,
        "suggested_tests": tests,
        "graph_limitations": graph_limitations(),
    });
    print_output(&output, format, render_detect_markdown(&output))
}

fn review_context(repo_root: &Path, db_path: &Path, task: &str, budget: u32, strict: bool, format: &str) -> Result<()> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let git = GitIntegration::new(repo_root);
    let config = ContextConfig {
        token_budget: budget,
        strict_mode: strict,
        max_files: 30,
        max_reverse_depth: 2,
        include_full_files: false,
        format: if is_json(format) { OutputFormat::Json } else { OutputFormat::Markdown },
    };
    let context = ContextBuilder::new(graph, config)
        .with_cache(db)
        .with_git(git)
        .build(task)?;
    if is_json(format) {
        print_json(&json!({
            "tool": "get_review_context",
            "context": context,
            "graph_limitations": graph_limitations(),
        }))
    } else {
        let renderer = ContextBuilder::new(CodeGraph::new(), ContextConfig::default());
        println!("{}", renderer.render_markdown(&context));
        Ok(())
    }
}

fn impact_radius(db_path: &Path, target: &str, depth: usize, format: &str) -> Result<()> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let starts = find_matching_nodes(&graph, target);
    let start_nodes = starts.iter().map(|n| NodeSummary::from(*n)).collect::<Vec<_>>();
    let start_paths = starts.iter().filter_map(|n| n.file_path.clone()).collect::<Vec<_>>();
    let impacted = walk_reverse_imports(&graph, &start_paths, depth);
    let tests = suggested_tests(&graph, &start_paths, 20);
    let output = json!({
        "tool": "get_impact_radius",
        "target": target,
        "depth": depth,
        "start_nodes": start_nodes,
        "impacted_files": impacted,
        "suggested_tests": tests,
        "graph_limitations": graph_limitations(),
    });
    print_output(&output, format, render_simple_markdown("Impact radius", &output))
}

fn affected_flows(repo_root: &Path, db_path: &Path, base: &str, head: &str, format: &str) -> Result<()> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let git = GitIntegration::new(repo_root);
    let changes = git.diff_files(base, head)?;
    let changed_files = changes.iter().map(|c| c.path.clone()).collect::<Vec<_>>();
    let impacted = walk_reverse_imports(&graph, &changed_files, 4);
    let flows = infer_flows(&graph, &changed_files, &impacted);
    let output = json!({
        "tool": "get_affected_flows",
        "base": base,
        "head": head,
        "changed_files": changed_files,
        "flows": flows,
        "graph_limitations": graph_limitations(),
    });
    print_output(&output, format, render_simple_markdown("Affected flows", &output))
}

fn query_graph(db_path: &Path, query: &str, format: &str) -> Result<()> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let (mode, nodes, edges) = run_graph_query(&graph, query);
    let output = json!({
        "tool": "query_graph",
        "query": query,
        "mode": mode,
        "nodes": nodes,
        "edges": edges,
    });
    print_output(&output, format, render_simple_markdown("Graph query", &output))
}

fn semantic_search(db_path: &Path, query: &str, kind: Option<&str>, limit: usize, format: &str) -> Result<()> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let hits = search_nodes(&graph, query, kind, limit);
    let output = json!({
        "tool": "semantic_search_nodes",
        "query": query,
        "kind": kind,
        "hits": hits,
    });
    print_output(&output, format, render_simple_markdown("Semantic search", &output))
}

fn architecture_overview(db_path: &Path, format: &str) -> Result<()> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let files = db.get_all_files()?;
    let cycles = graph
        .detect_cycles()
        .into_iter()
        .map(|cycle| {
            cycle
                .into_iter()
                .filter_map(|id| graph.get_node(id))
                .map(display_node)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let output = json!({
        "tool": "get_architecture_overview",
        "files_indexed": db.file_count()?,
        "symbols_indexed": db.symbol_count()?,
        "imports_indexed": db.import_count()?,
        "resolved_import_edges": db.edge_count()?,
        "languages": db.language_stats()?,
        "modules": summarize_modules(&files),
        "top_fan_in": top_degrees(&graph, Direction::Incoming, 10),
        "top_fan_out": top_degrees(&graph, Direction::Outgoing, 10),
        "cycles": cycles,
        "graph_limitations": graph_limitations(),
    });
    print_output(&output, format, render_arch_markdown(&output))
}

fn refactor_tool(db_path: &Path, action: &str, target: Option<&str>, replacement: Option<&str>, format: &str) -> Result<()> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let output = match action.to_lowercase().as_str() {
        "dead-code" | "dead_code" | "deadcode" => {
            let candidates = graph
                .nodes
                .values()
                .filter(|n| n.kind == NodeKind::File)
                .filter_map(|node| node.file_path.clone())
                .filter(|path| !is_test_file(path) && !is_config_or_doc(path))
                .filter(|path| fan_in(&graph, path) == 0 && fan_out(&graph, path) == 0)
                .take(100)
                .map(|path| json!({
                    "path": path,
                    "confidence": "low",
                    "reason": "No indexed import edges in or out. Dynamic usage is not ruled out."
                }))
                .collect::<Vec<_>>();
            json!({
                "tool": "refactor_tool",
                "action": "dead-code",
                "candidates": candidates,
                "warnings": ["Treat as candidates until call/reference edges are indexed."]
            })
        }
        "rename" => {
            let target = target.ok_or_else(|| anyhow!("rename requires a target"))?;
            let matches = find_matching_nodes(&graph, target)
                .into_iter()
                .map(NodeSummary::from)
                .collect::<Vec<_>>();
            let start_paths = matches.iter().filter_map(|n| n.file_path.clone()).collect::<Vec<_>>();
            json!({
                "tool": "refactor_tool",
                "action": "rename",
                "target": target,
                "replacement": replacement,
                "matches": matches,
                "impacted_files": walk_reverse_imports(&graph, &start_paths, 3),
                "warnings": ["Dry-run only. This command does not rewrite files."]
            })
        }
        "split-file" | "split_file" => {
            let target = target.ok_or_else(|| anyhow!("split-file requires a target file"))?;
            let symbols = graph
                .nodes
                .values()
                .filter(|n| n.kind != NodeKind::File && n.file_path.as_deref() == Some(target))
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
                    "Move private helpers first.",
                    "Keep public exports stable until callers are migrated."
                ]
            })
        }
        other => return Err(anyhow!("unknown refactor action: {}", other)),
    };
    print_output(&output, format, render_simple_markdown("Refactor plan", &output))
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
        let Some(&graph_file_id) = db_file_to_graph_node.get(&file.id) else { continue };
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

    for edge in db.get_edges_by_kind(EdgeKind::Imports)? {
        let Some(&source_graph_id) = db_file_to_graph_node.get(&edge.source_file_id) else { continue };
        let Some(target_file_id) = edge.target_file_id else { continue };
        let Some(&target_graph_id) = db_file_to_graph_node.get(&target_file_id) else { continue };
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

fn run_graph_query(graph: &CodeGraph, query: &str) -> (String, Vec<NodeSummary>, Vec<EdgeSummary>) {
    let mut parts = query.splitn(2, ' ');
    let op = parts.next().unwrap_or("").trim();
    let target = parts.next().unwrap_or("").trim();
    match op {
        "imports_of" | "deps_of" | "callees_of" => edge_query(graph, target, Direction::Outgoing, op),
        "imported_by" | "rdeps_of" | "callers_of" => edge_query(graph, target, Direction::Incoming, op),
        "symbols_in" => {
            let nodes = graph.nodes.values()
                .filter(|n| n.kind != NodeKind::File && n.file_path.as_deref() == Some(target))
                .map(NodeSummary::from)
                .collect::<Vec<_>>();
            (op.to_string(), nodes, Vec::new())
        }
        "tests_for" => {
            let paths = vec![target.to_string()];
            let nodes = suggested_tests(graph, &paths, 50)
                .into_iter()
                .filter_map(|p| file_node_by_path(graph, &p))
                .map(NodeSummary::from)
                .collect::<Vec<_>>();
            (op.to_string(), nodes, Vec::new())
        }
        _ => {
            let nodes = search_nodes(graph, query, None, 50).into_iter().map(|h| h.node).collect();
            ("search".to_string(), nodes, Vec::new())
        }
    }
}

fn edge_query(graph: &CodeGraph, target: &str, direction: Direction, mode: &str) -> (String, Vec<NodeSummary>, Vec<EdgeSummary>) {
    let start_nodes = find_matching_nodes(graph, target);
    let start_ids = start_nodes.iter().map(|n| n.id).collect::<HashSet<_>>();
    let edges = graph.edges.iter()
        .filter(|e| e.kind == EdgeKind::Imports || e.kind == EdgeKind::Calls)
        .filter(|e| match direction {
            Direction::Incoming => start_ids.contains(&e.target_id),
            Direction::Outgoing => start_ids.contains(&e.source_id),
        })
        .filter_map(|e| Some(EdgeSummary {
            source: NodeSummary::from(graph.get_node(e.source_id)?),
            target: NodeSummary::from(graph.get_node(e.target_id)?),
            kind: e.kind.as_str().to_string(),
        }))
        .collect::<Vec<_>>();
    (mode.to_string(), start_nodes.into_iter().map(NodeSummary::from).collect(), edges)
}

fn search_nodes(graph: &CodeGraph, query: &str, kind: Option<&str>, limit: usize) -> Vec<SearchHit> {
    let terms = search_terms(query);
    let kind = kind.map(str::to_lowercase);
    let query_lower = query.to_lowercase();
    let mut hits = graph.nodes.values()
        .filter(|node| kind.as_ref().map_or(true, |k| node.kind.as_str() == k))
        .filter_map(|node| {
            let mut score = 0.0;
            let name = node.name.to_lowercase();
            let path = node.file_path.as_deref().unwrap_or("").to_lowercase();
            let sig = node.signature.as_deref().unwrap_or("").to_lowercase();
            let haystack = format!("{} {} {} {}", name, path, sig, node.kind.as_str());
            if haystack.contains(&query_lower) { score += 5.0; }
            for term in &terms {
                if name.contains(term) { score += 3.0; }
                if path.contains(term) { score += 2.0; }
                if sig.contains(term) { score += 1.0; }
            }
            (score > 0.0).then(|| SearchHit { score, node: NodeSummary::from(node) })
        })
        .collect::<Vec<_>>();
    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(limit);
    hits
}

fn find_matching_nodes<'a>(graph: &'a CodeGraph, selector: &str) -> Vec<&'a GraphNode> {
    let selector_lower = selector.to_lowercase();
    let mut nodes = graph.nodes.values()
        .filter(|node| {
            node.name.to_lowercase().contains(&selector_lower)
                || node.file_path.as_deref().map_or(false, |p| p == selector || p.to_lowercase().contains(&selector_lower))
        })
        .collect::<Vec<_>>();
    nodes.sort_by_key(|n| (n.file_path.clone().unwrap_or_default(), n.start_line.unwrap_or(0)));
    nodes
}

fn walk_reverse_imports(graph: &CodeGraph, start_paths: &[String], max_depth: usize) -> Vec<ImpactedFile> {
    let start_set = start_paths.iter().cloned().collect::<HashSet<_>>();
    let mut queue = VecDeque::new();
    let mut visited: HashMap<u64, usize> = HashMap::new();
    for path in start_paths {
        if let Some(node) = file_node_by_path(graph, path) {
            visited.insert(node.id, 0);
            queue.push_back((node.id, 0));
        }
    }
    while let Some((node_id, depth)) = queue.pop_front() {
        if depth >= max_depth { continue; }
        for edge in graph.edges.iter().filter(|e| e.kind == EdgeKind::Imports && e.target_id == node_id) {
            if let std::collections::hash_map::Entry::Vacant(entry) = visited.entry(edge.source_id) {
                entry.insert(depth + 1);
                queue.push_back((edge.source_id, depth + 1));
            }
        }
    }
    let mut files = visited.into_iter()
        .filter_map(|(id, depth)| {
            let path = graph.get_node(id)?.file_path.clone()?;
            let reason = if start_set.contains(&path) { "changed_or_target_file" } else { "reverse_import_dependency" };
            Some(ImpactedFile { path, depth, reason: reason.to_string() })
        })
        .collect::<Vec<_>>();
    files.sort_by(|a, b| a.depth.cmp(&b.depth).then_with(|| a.path.cmp(&b.path)));
    files
}

fn infer_flows(graph: &CodeGraph, changed_paths: &[String], impacted: &[ImpactedFile]) -> Vec<FlowSummary> {
    let affected = impacted.iter().map(|f| f.path.clone()).chain(changed_paths.iter().cloned()).collect::<HashSet<_>>();
    let mut flows = affected.iter()
        .filter_map(|path| classify_flow(path).map(|kind| FlowSummary {
            entrypoint: path.clone(),
            kind,
            reason: if changed_paths.contains(path) { "entrypoint changed directly" } else { "entrypoint imports changed code" }.to_string(),
            touched_files: changed_paths.to_vec(),
        }))
        .collect::<Vec<_>>();
    if flows.is_empty() {
        for node in graph.nodes.values().filter(|n| n.kind == NodeKind::File) {
            let Some(path) = node.file_path.as_ref() else { continue };
            let Some(kind) = classify_flow(path) else { continue };
            let imports_changed = graph.edges.iter()
                .filter(|e| e.kind == EdgeKind::Imports && e.source_id == node.id)
                .filter_map(|e| graph.get_node(e.target_id))
                .filter_map(|target| target.file_path.as_ref())
                .any(|p| changed_paths.contains(p));
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
    flows.sort_by(|a, b| a.entrypoint.cmp(&b.entrypoint));
    flows.dedup_by(|a, b| a.entrypoint == b.entrypoint);
    flows
}

fn classify_flow(path: &str) -> Option<String> {
    let lower = path.to_lowercase();
    if lower.ends_with("/route.ts") || lower.ends_with("/route.tsx") || lower.ends_with("/route.js") || lower.contains("/pages/api/") {
        Some("nextjs_api_route".to_string())
    } else if lower.ends_with("/page.tsx") || lower.ends_with("/page.ts") || lower.ends_with("/page.jsx") {
        Some("nextjs_page".to_string())
    } else if lower == "src/main.rs" || lower.ends_with("/src/main.rs") || lower.contains("/src/bin/") {
        Some("rust_binary".to_string())
    } else if lower.contains("worker") || lower.contains("consumer") || lower.contains("queue") {
        Some("worker_or_consumer".to_string())
    } else if is_test_file(path) {
        Some("test_flow".to_string())
    } else {
        None
    }
}

fn risk_reasons(graph: &CodeGraph, changes: &[GitChange], impacted: &[ImpactedFile], flows: &[FlowSummary], tests: &[String]) -> Vec<String> {
    let mut reasons = Vec::new();
    if impacted.len() > 20 { reasons.push(format!("Wide blast radius: {} indexed files are affected.", impacted.len())); }
    if !flows.is_empty() { reasons.push(format!("{} entrypoint-like flow(s) are affected.", flows.len())); }
    if tests.is_empty() && changes.iter().any(|c| !is_test_file(&c.path)) {
        reasons.push("Source files changed but no indexed test candidates were found.".to_string());
    }
    for change in changes {
        let fan_in_count = fan_in(graph, &change.path);
        if fan_in_count > 5 { reasons.push(format!("{} has high fan-in: {} dependents.", change.path, fan_in_count)); }
        if is_risky_path(&change.path) { reasons.push(format!("{} touches a high-risk path or concern.", change.path)); }
    }
    reasons.sort();
    reasons.dedup();
    reasons
}

fn risk_score(changes: &[GitChange], impacted: &[ImpactedFile], flows: &[FlowSummary], reasons: &[String]) -> u32 {
    let changed_lines = changes.iter().map(|c| c.lines_added + c.lines_deleted).sum::<i64>();
    let mut score = 0u32;
    score += (changes.len() as u32).saturating_mul(5).min(25);
    score += ((changed_lines / 40).max(0) as u32).min(20);
    score += (impacted.len() as u32).saturating_mul(2).min(25);
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

fn suggested_tests(graph: &CodeGraph, paths: &[String], limit: usize) -> Vec<String> {
    let terms = paths.iter().flat_map(|p| search_terms(p)).filter(|t| t.len() > 3).collect::<HashSet<_>>();
    let mut scored = graph.nodes.values()
        .filter_map(|n| n.file_path.as_ref())
        .filter(|p| is_test_file(p))
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
    let mut degrees = graph.nodes.values()
        .filter(|n| n.kind == NodeKind::File)
        .filter_map(|n| {
            let path = n.file_path.clone()?;
            let count = match direction { Direction::Incoming => fan_in(graph, &path), Direction::Outgoing => fan_out(graph, &path) };
            (count > 0).then_some(FileDegree { path, count })
        })
        .collect::<Vec<_>>();
    degrees.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.path.cmp(&b.path)));
    degrees.truncate(limit);
    degrees
}

fn summarize_modules(files: &[CachedFile]) -> Vec<ModuleSummary> {
    let mut modules: BTreeMap<String, (usize, i64)> = BTreeMap::new();
    for file in files {
        let module = module_name(&file.path);
        let entry = modules.entry(module).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += file.loc;
    }
    let mut output = modules.into_iter().map(|(module, (files, loc))| ModuleSummary { module, files, loc }).collect::<Vec<_>>();
    output.sort_by(|a, b| b.files.cmp(&a.files).then_with(|| a.module.cmp(&b.module)));
    output
}

fn module_name(path: &str) -> String {
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.len() >= 2 && parts[0] == "crates" { format!("crates/{}", parts[1]) } else { parts.first().copied().unwrap_or("root").to_string() }
}

fn fan_in(graph: &CodeGraph, path: &str) -> usize {
    file_node_by_path(graph, path)
        .map(|node| graph.edges.iter().filter(|e| e.kind == EdgeKind::Imports && e.target_id == node.id).count())
        .unwrap_or(0)
}

fn fan_out(graph: &CodeGraph, path: &str) -> usize {
    file_node_by_path(graph, path)
        .map(|node| graph.edges.iter().filter(|e| e.kind == EdgeKind::Imports && e.source_id == node.id).count())
        .unwrap_or(0)
}

fn file_node_by_path<'a>(graph: &'a CodeGraph, path: &str) -> Option<&'a GraphNode> {
    graph.nodes.values().find(|n| n.kind == NodeKind::File && n.file_path.as_deref() == Some(path))
}

fn display_node(node: &GraphNode) -> String {
    node.file_path.clone().unwrap_or_else(|| node.name.clone())
}

fn search_terms(input: &str) -> Vec<String> {
    input.split(|ch: char| !ch.is_alphanumeric()).filter(|p| p.len() > 2).map(|p| p.to_lowercase()).collect()
}

fn is_test_file(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains("test") || lower.contains("spec") || lower.contains("__tests__")
}

fn is_config_or_doc(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".md") || lower.ends_with(".json") || lower.ends_with(".toml") || lower.ends_with(".yaml") || lower.ends_with(".yml") || lower.contains("config")
}

fn is_risky_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    ["auth", "session", "payment", "stripe", "billing", "db", "database", "schema", "migration", "security", "permission", "route", "api", "env", "config"]
        .iter()
        .any(|needle| lower.contains(needle))
}

fn graph_limitations() -> Vec<String> {
    vec![
        "Compatibility output currently uses indexed files, symbols, imports, and resolved import edges.".to_string(),
        "Precise function-level call-flow output requires future call-edge indexing.".to_string(),
        "Dynamic imports, reflection, and framework runtime behavior may be under-approximated.".to_string(),
    ]
}

fn is_json(format: &str) -> bool {
    format.eq_ignore_ascii_case("json")
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn print_output(value: &serde_json::Value, format: &str, markdown: String) -> Result<()> {
    if is_json(format) {
        print_json(value)
    } else {
        println!("{}", markdown);
        Ok(())
    }
}

fn render_detect_markdown(value: &serde_json::Value) -> String {
    format!("# Change risk analysis\n\n```json\n{}\n```", serde_json::to_string_pretty(value).unwrap_or_default())
}

fn render_arch_markdown(value: &serde_json::Value) -> String {
    format!("# Architecture overview\n\n```json\n{}\n```", serde_json::to_string_pretty(value).unwrap_or_default())
}

fn render_simple_markdown(title: &str, value: &serde_json::Value) -> String {
    format!("# {}\n\n```json\n{}\n```", title, serde_json::to_string_pretty(value).unwrap_or_default())
}
