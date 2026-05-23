use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::env;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use reposcry_cache::db::{CacheDb, CachedFile};
use reposcry_context::{ContextBuilder, ContextConfig, OutputFormat};
use reposcry_git::{GitChange, GitIntegration};
use reposcry_graph::edge::EdgeKind;
use reposcry_graph::graph::CodeGraph;
use reposcry_graph::node::{GraphNode, NodeKind};
use serde::Serialize;
use serde_json::{json, Value};

const CACHE_DIR: &str = ".reposcry";
const CACHE_DB: &str = "reposcry.db";
const LOCAL_SEMANTIC_BACKEND: &str = "local-hash-v1";
const LOCAL_SEMANTIC_DIMS: usize = 64;
const OLLAMA_BACKEND: &str = "ollama";

#[derive(Parser)]
#[command(
    name = "reposcry-crg",
    version,
    about = "CRG-compatible analysis commands for RepoScry"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(global = true, long = "repo", short = 'C', default_value = ".")]
    repo_root: String,
}

#[derive(Subcommand)]
pub enum Commands {
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
        #[arg(long, default_value_t = false)]
        no_runtime_calls: bool,
        #[arg(long, default_value = "json")]
        format: String,
    },
    /// Finding functions/classes/files by name, path, signature, or keyword.
    #[command(
        name = "semantic_search_nodes",
        visible_alias = "semantic-search-nodes"
    )]
    SemanticSearchNodes {
        query: String,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long, default_value = "20")]
        limit: usize,
        #[arg(long, default_value_t = false)]
        semantic: bool,
        #[arg(long)]
        semantic_backend: Option<String>,
        #[arg(long, default_value = "json")]
        format: String,
    },
    /// Understanding high-level codebase structure.
    #[command(
        name = "get_architecture_overview",
        visible_alias = "get-architecture-overview"
    )]
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
    /// Run a minimal MCP-compatible stdio server.
    Mcp {
        #[arg(long, default_value_t = 1_048_576)]
        max_request_bytes: usize,
    },
}

#[derive(Debug, Clone)]
struct ToolCallSpec {
    name: &'static str,
    description: &'static str,
    input_schema: Value,
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
    path: Vec<String>,
    changed_symbols: Vec<String>,
    risk: String,
    confidence: String,
    reason: String,
    touched_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct SearchHit {
    score: f64,
    match_reason: String,
    node: NodeSummary,
}

#[derive(Debug, Clone, Serialize)]
struct SuggestedTest {
    path: String,
    command: String,
    confidence: String,
    score: f64,
    reasons: Vec<String>,
    test_symbols: Vec<String>,
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

    run_command(&repo_root, &db_path, cli.command)
}

pub fn run_command(repo_root: &Path, db_path: &Path, command: Commands) -> Result<()> {
    match command {
        Commands::DetectChanges { base, head, format } => {
            detect_changes(&repo_root, &db_path, &base, &head, &format)
        }
        Commands::GetReviewContext {
            task,
            budget,
            strict,
            format,
        } => review_context(&repo_root, &db_path, &task, budget, strict, &format),
        Commands::GetImpactRadius {
            target,
            depth,
            format,
        } => impact_radius(&db_path, &target, depth, &format),
        Commands::GetAffectedFlows { base, head, format } => {
            affected_flows(&repo_root, &db_path, &base, &head, &format)
        }
        Commands::QueryGraph {
            query,
            no_runtime_calls: _,
            format,
        } => query_graph(&db_path, &query, &format),
        Commands::SemanticSearchNodes {
            query,
            kind,
            limit,
            semantic,
            semantic_backend,
            format,
        } => semantic_search(
            &db_path,
            &query,
            kind.as_deref(),
            limit,
            semantic,
            semantic_backend.as_deref(),
            &format,
        ),
        Commands::GetArchitectureOverview { format } => architecture_overview(&db_path, &format),
        Commands::RefactorTool {
            action,
            target,
            replacement,
            format,
        } => refactor_tool(
            &db_path,
            &action,
            target.as_deref(),
            replacement.as_deref(),
            &format,
        ),
        Commands::Mcp { max_request_bytes } => run_mcp(&repo_root, &db_path, max_request_bytes),
    }
}

fn run_mcp(repo_root: &Path, db_path: &Path, max_request_bytes: usize) -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = process_mcp_line(repo_root, db_path, &line, max_request_bytes);
        if let Some(response) = response {
            writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
            stdout.flush()?;
        }
    }
    Ok(())
}

fn process_mcp_line(
    repo_root: &Path,
    db_path: &Path,
    line: &str,
    max_request_bytes: usize,
) -> Option<Value> {
    if line.len() > max_request_bytes {
        return Some(mcp_error_value(
            None,
            -32000,
            "request too large",
            Some(json!({ "max_request_bytes": max_request_bytes })),
        ));
    }

    let request: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => {
            mcp_log(&format!("invalid JSON request: {}", error));
            return Some(mcp_error_value(
                None,
                -32700,
                "parse error",
                Some(json!({ "details": error.to_string() })),
            ));
        }
    };

    process_mcp_request(repo_root, db_path, request)
}

fn process_mcp_request(repo_root: &Path, db_path: &Path, request: Value) -> Option<Value> {
    let id = request.get("id").cloned();
    let Some(method) = request.get("method").and_then(Value::as_str) else {
        return Some(mcp_error_value(
            id,
            -32600,
            "invalid request",
            Some(json!({ "details": "missing method" })),
        ));
    };
    let params = request.get("params").cloned().unwrap_or_else(|| json!({}));

    let response = match method {
        "initialize" => mcp_success_value(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {
                        "listChanged": false
                    }
                },
                "serverInfo": {
                    "name": "reposcry",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        ),
        "notifications/initialized" => return None,
        "tools/list" => mcp_success_value(
            id,
            json!({
                "tools": mcp_tools()
                    .into_iter()
                    .map(|tool| json!({
                        "name": tool.name,
                        "description": tool.description,
                        "inputSchema": tool.input_schema,
                    }))
                    .collect::<Vec<_>>()
            }),
        ),
        "tools/call" => match mcp_tool_call(repo_root, db_path, &params) {
            Ok(result) => mcp_success_value(
                id,
                json!({
                    "content": [
                        {
                            "type": "text",
                            "text": serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string())
                        }
                    ],
                    "isError": false
                }),
            ),
            Err(error) => mcp_error_value(
                id,
                -32001,
                "tool call failed",
                Some(json!({ "details": error.to_string() })),
            ),
        },
        other => mcp_error_value(
            id,
            -32601,
            "method not found",
            Some(json!({ "method": other })),
        ),
    };

    Some(response)
}

fn mcp_success_value(id: Option<Value>, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "result": result,
    })
}

fn mcp_error_value(id: Option<Value>, code: i64, message: &str, data: Option<Value>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": {
            "code": code,
            "message": message,
            "data": data.unwrap_or(Value::Null),
        }
    })
}

fn mcp_log(message: &str) {
    let _ = writeln!(io::stderr().lock(), "reposcry mcp: {}", message);
}

fn mcp_tools() -> Vec<ToolCallSpec> {
    vec![
        ToolCallSpec {
            name: "detect_changes",
            description: "Review code changes and return risk-scored analysis.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "base": {"type": "string"},
                    "head": {"type": "string"}
                },
                "required": ["base"]
            }),
        },
        ToolCallSpec {
            name: "get_review_context",
            description: "Build a focused code review context pack.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "task": {"type": "string"},
                    "budget": {"type": "integer"},
                    "strict": {"type": "boolean"}
                },
                "required": ["task"]
            }),
        },
        ToolCallSpec {
            name: "get_impact_radius",
            description: "Understand the blast radius of a file or symbol.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "target": {"type": "string"},
                    "depth": {"type": "integer"}
                },
                "required": ["target"]
            }),
        },
        ToolCallSpec {
            name: "get_affected_flows",
            description: "Return behavior-level entrypoint flows touched by a change.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "base": {"type": "string"},
                    "head": {"type": "string"}
                },
                "required": ["base"]
            }),
        },
        ToolCallSpec {
            name: "query_graph",
            description: "Trace callers, callees, imports, tests, and dependencies.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"}
                },
                "required": ["query"]
            }),
        },
        ToolCallSpec {
            name: "semantic_search_nodes",
            description: "Search files and symbols by path, signature, and keywords.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "kind": {"type": "string"},
                    "limit": {"type": "integer"},
                    "semantic": {"type": "boolean"},
                    "semantic_backend": {"type": "string"}
                },
                "required": ["query"]
            }),
        },
        ToolCallSpec {
            name: "get_architecture_overview",
            description: "Return a high-level architecture summary for the repository.",
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolCallSpec {
            name: "refactor_tool",
            description: "Plan rename, dead-code, split-file, or public API change refactors.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": {"type": "string"},
                    "target": {"type": "string"},
                    "replacement": {"type": "string"}
                }
            }),
        },
    ]
}

fn mcp_tool_call(repo_root: &Path, db_path: &Path, params: &Value) -> Result<Value> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing tool name"))?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    match name {
        "detect_changes" => detect_changes_value(
            repo_root,
            db_path,
            required_string(&args, "base")?,
            optional_string(&args, "head").unwrap_or("HEAD"),
        ),
        "get_review_context" => review_context_value(
            repo_root,
            db_path,
            required_string(&args, "task")?,
            optional_u32(&args, "budget").unwrap_or(20_000),
            optional_bool(&args, "strict").unwrap_or(false),
        ),
        "get_impact_radius" => impact_radius_value(
            db_path,
            required_string(&args, "target")?,
            optional_usize(&args, "depth").unwrap_or(3),
        ),
        "get_affected_flows" => affected_flows_value(
            repo_root,
            db_path,
            required_string(&args, "base")?,
            optional_string(&args, "head").unwrap_or("HEAD"),
        ),
        "query_graph" => query_graph_value(db_path, required_string(&args, "query")?),
        "semantic_search_nodes" => semantic_search_value(
            db_path,
            required_string(&args, "query")?,
            args.get("kind").and_then(Value::as_str),
            optional_usize(&args, "limit").unwrap_or(20),
            optional_bool(&args, "semantic").unwrap_or(false),
            args.get("semantic_backend").and_then(Value::as_str),
        ),
        "get_architecture_overview" => architecture_overview_value(db_path),
        "refactor_tool" => refactor_tool_value(
            repo_root,
            db_path,
            optional_string(&args, "action").unwrap_or("dead-code"),
            args.get("target").and_then(Value::as_str),
            args.get("replacement").and_then(Value::as_str),
        ),
        other => Err(anyhow!("unknown tool: {}", other)),
    }
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required string argument: {}", key))
}

fn optional_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn optional_u32(value: &Value, key: &str) -> Option<u32> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
}

fn optional_usize(value: &Value, key: &str) -> Option<usize> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
}

fn optional_bool(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

fn detect_changes(
    repo_root: &Path,
    db_path: &Path,
    base: &str,
    head: &str,
    format: &str,
) -> Result<()> {
    let output = detect_changes_value(repo_root, db_path, base, head)?;
    print_output(&output, format, render_detect_markdown(&output))
}

fn detect_changes_value(repo_root: &Path, db_path: &Path, base: &str, head: &str) -> Result<Value> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let git = GitIntegration::new(repo_root);
    let changes = git.diff_files(base, head)?;
    let changed_paths = changes.iter().map(|c| c.path.clone()).collect::<Vec<_>>();
    let impacted = walk_reverse_imports(&graph, &changed_paths, 3);
    let flows = infer_flows(&graph, &changed_paths, &impacted);
    let tests = suggested_tests_for_paths(&graph, &changed_paths, 20);
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

    Ok(json!({
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
    }))
}

fn review_context(
    repo_root: &Path,
    db_path: &Path,
    task: &str,
    budget: u32,
    strict: bool,
    format: &str,
) -> Result<()> {
    let output = review_context_value(repo_root, db_path, task, budget, strict)?;
    print_output(
        &output,
        format,
        render_simple_markdown("Review context", &output),
    )
}

fn review_context_value(
    repo_root: &Path,
    db_path: &Path,
    task: &str,
    budget: u32,
    strict: bool,
) -> Result<Value> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let git = GitIntegration::new(repo_root);
    let config = ContextConfig {
        token_budget: budget,
        strict_mode: strict,
        max_files: 30,
        max_reverse_depth: 2,
        include_full_files: false,
        format: OutputFormat::Json,
    };
    let context = ContextBuilder::new(graph, config)
        .with_cache(db)
        .with_git(git)
        .build(task)?;
    Ok(json!({
        "tool": "get_review_context",
        "context": context,
        "graph_limitations": graph_limitations(),
    }))
}

fn impact_radius(db_path: &Path, target: &str, depth: usize, format: &str) -> Result<()> {
    let output = impact_radius_value(db_path, target, depth)?;
    print_output(
        &output,
        format,
        render_simple_markdown("Impact radius", &output),
    )
}

fn impact_radius_value(db_path: &Path, target: &str, depth: usize) -> Result<Value> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let starts = find_matching_nodes(&graph, target);
    let start_nodes = starts
        .iter()
        .map(|n| NodeSummary::from(*n))
        .collect::<Vec<_>>();
    let start_paths = starts
        .iter()
        .filter_map(|n| n.file_path.clone())
        .collect::<Vec<_>>();
    let impacted = walk_reverse_imports(&graph, &start_paths, depth);
    let tests = suggested_tests_for_selector(&graph, target, 20);
    Ok(json!({
        "tool": "get_impact_radius",
        "target": target,
        "depth": depth,
        "start_nodes": start_nodes,
        "impacted_files": impacted,
        "suggested_tests": tests,
        "graph_limitations": graph_limitations(),
    }))
}

fn affected_flows(
    repo_root: &Path,
    db_path: &Path,
    base: &str,
    head: &str,
    format: &str,
) -> Result<()> {
    let output = affected_flows_value(repo_root, db_path, base, head)?;
    print_output(
        &output,
        format,
        render_simple_markdown("Affected flows", &output),
    )
}

fn affected_flows_value(repo_root: &Path, db_path: &Path, base: &str, head: &str) -> Result<Value> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let git = GitIntegration::new(repo_root);
    let changes = git.diff_files(base, head)?;
    let changed_files = changes.iter().map(|c| c.path.clone()).collect::<Vec<_>>();
    let impacted = walk_reverse_imports(&graph, &changed_files, 4);
    let flows = infer_flows(&graph, &changed_files, &impacted);
    Ok(json!({
        "tool": "get_affected_flows",
        "base": base,
        "head": head,
        "changed_files": changed_files,
        "flows": flows,
        "graph_limitations": graph_limitations(),
    }))
}

fn query_graph(db_path: &Path, query: &str, format: &str) -> Result<()> {
    let output = query_graph_value(db_path, query)?;
    print_output(
        &output,
        format,
        render_simple_markdown("Graph query", &output),
    )
}

fn query_graph_value(db_path: &Path, query: &str) -> Result<Value> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    if let Some(target) = query.strip_prefix("tests_for ").map(str::trim) {
        return Ok(json!({
            "tool": "query_graph",
            "query": query,
            "mode": "tests_for",
            "target": target,
            "suggested_tests": suggested_tests_for_selector(&graph, target, 50),
        }));
    }
    let (mode, nodes, edges) = run_graph_query(&graph, query);
    Ok(json!({
        "tool": "query_graph",
        "query": query,
        "mode": mode,
        "nodes": nodes,
        "edges": edges,
    }))
}

fn semantic_search(
    db_path: &Path,
    query: &str,
    kind: Option<&str>,
    limit: usize,
    semantic: bool,
    semantic_backend: Option<&str>,
    format: &str,
) -> Result<()> {
    let output = semantic_search_value(db_path, query, kind, limit, semantic, semantic_backend)?;
    print_output(
        &output,
        format,
        render_simple_markdown("Semantic search", &output),
    )
}

fn semantic_search_value(
    db_path: &Path,
    query: &str,
    kind: Option<&str>,
    limit: usize,
    semantic: bool,
    semantic_backend: Option<&str>,
) -> Result<Value> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let base_hits = match db.search_nodes_fts(query, kind, limit) {
        Ok(fts_hits) if !fts_hits.is_empty() => fts_hits
            .into_iter()
            .map(|hit| SearchHit {
                score: hit.score,
                match_reason: hit.match_reason,
                node: NodeSummary {
                    id: u64::try_from(hit.node_id.max(0)).unwrap_or_default(),
                    name: hit.name,
                    kind: hit.kind,
                    file_path: Some(hit.file_path),
                    line: None,
                    signature: hit.signature,
                },
            })
            .collect(),
        _ => search_nodes(&graph, query, kind, limit),
    };
    let resolved_backend = resolved_semantic_backend(&db, semantic_backend);
    let hits = if semantic {
        hybrid_search_nodes(
            &db,
            &graph,
            query,
            kind,
            limit,
            base_hits,
            &resolved_backend,
        )?
    } else {
        base_hits
    };
    Ok(json!({
        "tool": "semantic_search_nodes",
        "query": query,
        "kind": kind,
        "semantic": semantic,
        "semantic_backend": resolved_backend,
        "hits": hits,
    }))
}

fn architecture_overview(db_path: &Path, format: &str) -> Result<()> {
    let output = architecture_overview_value(db_path)?;
    print_output(&output, format, render_arch_markdown(&output))
}

fn architecture_overview_value(db_path: &Path) -> Result<Value> {
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
    Ok(json!({
        "tool": "get_architecture_overview",
        "files_indexed": db.file_count()?,
        "symbols_indexed": db.symbol_count()?,
        "imports_indexed": db.import_count()?,
        "persisted_call_sites": db.call_site_count()?,
        "persisted_symbol_call_edges": db.symbol_edge_count()?,
        "persisted_file_call_edges": db.get_edges_by_kind(EdgeKind::Calls)?.len(),
        "resolved_import_edges": db.edge_count()?,
        "languages": db.language_stats()?,
        "modules": summarize_modules(&files),
        "top_fan_in": top_degrees(&graph, Direction::Incoming, 10),
        "top_fan_out": top_degrees(&graph, Direction::Outgoing, 10),
        "cycles": cycles,
        "graph_limitations": graph_limitations(),
    }))
}

fn refactor_tool(
    db_path: &Path,
    action: &str,
    target: Option<&str>,
    replacement: Option<&str>,
    format: &str,
) -> Result<()> {
    let repo_root = db_path
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| anyhow!("could not infer repo root from db path"))?;
    let output = refactor_tool_value(repo_root, db_path, action, target, replacement)?;
    print_output(
        &output,
        format,
        render_simple_markdown("Refactor plan", &output),
    )
}

fn refactor_tool_value(
    repo_root: &Path,
    db_path: &Path,
    action: &str,
    target: Option<&str>,
    replacement: Option<&str>,
) -> Result<Value> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    match action.to_lowercase().as_str() {
        "dead-code" | "dead_code" | "deadcode" => {
            let candidates = dead_code_candidates(&graph);
            Ok(json!({
                "tool": "refactor_tool",
                "action": "dead-code",
                "candidates": {
                    "high": candidates.0,
                    "medium": candidates.1,
                    "low": candidates.2,
                    "public_api_risk": candidates.3,
                },
                "warnings": ["Treat as candidates until reference and runtime edges are fully indexed."]
            }))
        }
        "rename" => {
            let target = target.ok_or_else(|| anyhow!("rename requires a target"))?;
            let matches = find_matching_nodes(&graph, target);
            let match_summaries = matches
                .iter()
                .map(|node| NodeSummary::from(*node))
                .collect::<Vec<_>>();
            let start_paths = match_summaries
                .iter()
                .filter_map(|n| n.file_path.clone())
                .collect::<Vec<_>>();
            Ok(json!({
                "tool": "refactor_tool",
                "action": "rename",
                "target": target,
                "replacement": replacement,
                "matches": match_summaries,
                "direct_references": rename_reference_summary(&graph, &matches),
                "impacted_files": walk_reverse_imports(&graph, &start_paths, 3),
                "impacted_tests": suggested_tests_for_selector(&graph, target, 20),
                "warnings": ["Dry-run only. This command does not rewrite files."]
            }))
        }
        "split-file" | "split_file" => {
            let target = target.ok_or_else(|| anyhow!("split-file requires a target file"))?;
            let symbols = graph
                .nodes
                .values()
                .filter(|n| n.kind != NodeKind::File && n.file_path.as_deref() == Some(target))
                .map(NodeSummary::from)
                .collect::<Vec<_>>();
            Ok(json!({
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
            }))
        }
        "public-api-change" | "public_api_change" => {
            let base = target.ok_or_else(|| anyhow!("public-api-change requires a base ref"))?;
            let head = replacement.unwrap_or("HEAD");
            let git = GitIntegration::new(repo_root);
            let changes = git.diff_files(base, head)?;
            let changed_paths = changes
                .iter()
                .map(|change| change.path.clone())
                .collect::<HashSet<_>>();
            let public_symbols = graph
                .nodes
                .values()
                .filter(|node| node.kind != NodeKind::File)
                .filter(|node| {
                    node.file_path
                        .as_ref()
                        .map(|path| changed_paths.contains(path))
                        .unwrap_or(false)
                })
                .filter(|node| is_public_api_node(node))
                .map(|node| {
                    json!({
                        "symbol": NodeSummary::from(node),
                        "references": incoming_reference_count(&graph, node.id),
                        "impacted_tests": suggested_tests_for_selector(&graph, &node.name, 10),
                    })
                })
                .collect::<Vec<_>>();
            Ok(json!({
                "tool": "refactor_tool",
                "action": "public-api-change",
                "base": base,
                "head": head,
                "changed_files": changes,
                "public_symbols": public_symbols,
                "warnings": ["Symbol-level API compatibility is inferred from indexed visibility and graph references."]
            }))
        }
        other => Err(anyhow!("unknown refactor action: {}", other)),
    }
}

fn rebuild_graph(db: &CacheDb) -> Result<CodeGraph> {
    let files = db.get_all_files()?;
    let mut graph = CodeGraph::new();
    let mut db_file_to_graph_node: HashMap<i64, u64> = HashMap::new();
    let mut db_symbol_to_graph_node: HashMap<i64, u64> = HashMap::new();

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
            if let Some(db_symbol_id) = sym.id {
                db_symbol_to_graph_node.insert(db_symbol_id, sym_id);
            }
            graph.add_edge(graph_file_id, sym_id, EdgeKind::Contains);
        }
    }

    for edge in db.get_edges_by_kind(EdgeKind::Imports)? {
        let Some(&source_graph_id) = db_file_to_graph_node.get(&edge.source_file_id) else {
            continue;
        };
        let Some(target_file_id) = edge.target_file_id else {
            continue;
        };
        let Some(&target_graph_id) = db_file_to_graph_node.get(&target_file_id) else {
            continue;
        };
        graph.add_edge(source_graph_id, target_graph_id, EdgeKind::Imports);
    }
    for edge in db.get_edges_by_kind(EdgeKind::Calls)? {
        let Some(&source_graph_id) = db_file_to_graph_node.get(&edge.source_file_id) else {
            continue;
        };
        let Some(target_file_id) = edge.target_file_id else {
            continue;
        };
        let Some(&target_graph_id) = db_file_to_graph_node.get(&target_file_id) else {
            continue;
        };
        graph.add_edge(source_graph_id, target_graph_id, EdgeKind::Calls);
    }
    for edge in db.get_symbol_edges_by_kind(EdgeKind::Calls.as_str())? {
        let Some(&source_graph_id) = db_symbol_to_graph_node.get(&edge.source_symbol_id) else {
            continue;
        };
        let Some(&target_graph_id) = db_symbol_to_graph_node.get(&edge.target_symbol_id) else {
            continue;
        };
        graph.add_edge(source_graph_id, target_graph_id, EdgeKind::Calls);
    }
    Ok(graph)
}

fn dead_code_candidates(
    graph: &CodeGraph,
) -> (
    Vec<serde_json::Value>,
    Vec<serde_json::Value>,
    Vec<serde_json::Value>,
    Vec<serde_json::Value>,
) {
    let mut high = Vec::new();
    let mut medium = Vec::new();
    let mut low = Vec::new();
    let mut public_api_risk = Vec::new();

    for node in graph
        .nodes
        .values()
        .filter(|node| {
            node.kind != NodeKind::File
                && node.kind != NodeKind::Test
                && node.kind != NodeKind::Module
        })
        .filter(|node| {
            node.file_path
                .as_deref()
                .map(|path| !is_config_or_doc(path))
                .unwrap_or(true)
        })
    {
        let incoming_calls = incoming_callers(graph, node.id);
        let touched_by_tests = incoming_test_callers(graph, node.id) > 0;
        let exported = is_public_api_node(node);
        let entrypoint_like = node.file_path.as_deref().and_then(classify_flow).is_some();

        if !incoming_calls.is_empty() || touched_by_tests || entrypoint_like {
            continue;
        }

        let candidate = json!({
            "symbol": NodeSummary::from(node),
            "reason": dead_code_reason(graph, node.id, exported),
            "incoming_references": incoming_reference_count(graph, node.id),
        });

        if exported {
            public_api_risk.push(candidate);
        } else if node
            .file_path
            .as_deref()
            .map(|path| fan_in(graph, path) == 0)
            .unwrap_or(false)
        {
            high.push(candidate);
        } else if node.kind == NodeKind::Function || node.kind == NodeKind::Method {
            medium.push(candidate);
        } else {
            low.push(candidate);
        }
    }

    high.truncate(40);
    medium.truncate(40);
    low.truncate(40);
    public_api_risk.truncate(40);

    (high, medium, low, public_api_risk)
}

fn rename_reference_summary(graph: &CodeGraph, matches: &[&GraphNode]) -> Vec<serde_json::Value> {
    matches
        .iter()
        .map(|node| {
            let callers = incoming_callers(graph, node.id)
                .into_iter()
                .map(NodeSummary::from)
                .collect::<Vec<_>>();
            let callees = outgoing_callees(graph, node.id)
                .into_iter()
                .map(NodeSummary::from)
                .collect::<Vec<_>>();
            json!({
                "match": NodeSummary::from(*node),
                "incoming_callers": callers,
                "outgoing_callees": callees,
                "impacted_tests": suggested_tests_for_selector(graph, &node.name, 10),
            })
        })
        .collect()
}

fn incoming_callers<'a>(graph: &'a CodeGraph, node_id: u64) -> Vec<&'a GraphNode> {
    graph
        .edges
        .iter()
        .filter(|edge| edge.kind == EdgeKind::Calls && edge.target_id == node_id)
        .filter_map(|edge| graph.get_node(edge.source_id))
        .collect()
}

fn outgoing_callees<'a>(graph: &'a CodeGraph, node_id: u64) -> Vec<&'a GraphNode> {
    graph
        .edges
        .iter()
        .filter(|edge| edge.kind == EdgeKind::Calls && edge.source_id == node_id)
        .filter_map(|edge| graph.get_node(edge.target_id))
        .collect()
}

fn incoming_test_callers(graph: &CodeGraph, node_id: u64) -> usize {
    incoming_callers(graph, node_id)
        .into_iter()
        .filter(|node| node.kind == NodeKind::Test)
        .count()
}

fn incoming_reference_count(graph: &CodeGraph, node_id: u64) -> usize {
    graph
        .edges
        .iter()
        .filter(|edge| {
            edge.target_id == node_id
                && matches!(
                    edge.kind,
                    EdgeKind::Calls | EdgeKind::Imports | EdgeKind::References | EdgeKind::Tests
                )
        })
        .count()
}

fn dead_code_reason(graph: &CodeGraph, node_id: u64, exported: bool) -> String {
    if exported {
        "No indexed callers were found, but the symbol looks public and may be used externally."
            .to_string()
    } else {
        let refs = incoming_reference_count(graph, node_id);
        if refs == 0 {
            "No indexed callers or references were found.".to_string()
        } else {
            format!(
                "{} non-call graph reference(s) remain; manual confirmation is recommended.",
                refs
            )
        }
    }
}

fn is_public_api_node(node: &GraphNode) -> bool {
    node.visibility
        .as_deref()
        .map(|visibility| {
            visibility.contains("pub")
                || visibility.contains("public")
                || visibility.contains("export")
        })
        .unwrap_or(false)
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
        "imports_of" | "deps_of" | "callees_of" => {
            edge_query(graph, target, Direction::Outgoing, op)
        }
        "imported_by" | "rdeps_of" | "callers_of" => {
            edge_query(graph, target, Direction::Incoming, op)
        }
        "symbols_in" => {
            let nodes = graph
                .nodes
                .values()
                .filter(|n| n.kind != NodeKind::File && n.file_path.as_deref() == Some(target))
                .map(NodeSummary::from)
                .collect::<Vec<_>>();
            (op.to_string(), nodes, Vec::new())
        }
        _ => {
            let nodes = search_nodes(graph, query, None, 50)
                .into_iter()
                .map(|h| h.node)
                .collect();
            ("search".to_string(), nodes, Vec::new())
        }
    }
}

fn edge_query(
    graph: &CodeGraph,
    target: &str,
    direction: Direction,
    mode: &str,
) -> (String, Vec<NodeSummary>, Vec<EdgeSummary>) {
    let start_nodes = find_matching_nodes(graph, target);
    let start_ids = start_nodes.iter().map(|n| n.id).collect::<HashSet<_>>();
    let edges = graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Imports || e.kind == EdgeKind::Calls)
        .filter(|e| match direction {
            Direction::Incoming => start_ids.contains(&e.target_id),
            Direction::Outgoing => start_ids.contains(&e.source_id),
        })
        .filter_map(|e| {
            Some(EdgeSummary {
                source: NodeSummary::from(graph.get_node(e.source_id)?),
                target: NodeSummary::from(graph.get_node(e.target_id)?),
                kind: e.kind.as_str().to_string(),
            })
        })
        .collect::<Vec<_>>();
    (
        mode.to_string(),
        start_nodes.into_iter().map(NodeSummary::from).collect(),
        edges,
    )
}

fn search_nodes(
    graph: &CodeGraph,
    query: &str,
    kind: Option<&str>,
    limit: usize,
) -> Vec<SearchHit> {
    let terms = search_terms(query);
    let kind = kind.map(str::to_lowercase);
    let query_lower = query.to_lowercase();
    let mut hits = graph
        .nodes
        .values()
        .filter(|node| kind.as_ref().map_or(true, |k| node.kind.as_str() == k))
        .filter_map(|node| {
            let mut score = 0.0;
            let name = node.name.to_lowercase();
            let path = node.file_path.as_deref().unwrap_or("").to_lowercase();
            let sig = node.signature.as_deref().unwrap_or("").to_lowercase();
            let haystack = format!("{} {} {} {}", name, path, sig, node.kind.as_str());
            if haystack.contains(&query_lower) {
                score += 5.0;
            }
            for term in &terms {
                if name.contains(term) {
                    score += 3.0;
                }
                if path.contains(term) {
                    score += 2.0;
                }
                if sig.contains(term) {
                    score += 1.0;
                }
            }
            (score > 0.0).then(|| SearchHit {
                score,
                match_reason: "substring_fallback".to_string(),
                node: NodeSummary::from(node),
            })
        })
        .collect::<Vec<_>>();
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(limit);
    hits
}

fn hybrid_search_nodes(
    db: &CacheDb,
    graph: &CodeGraph,
    query: &str,
    kind: Option<&str>,
    limit: usize,
    base_hits: Vec<SearchHit>,
    semantic_backend: &str,
) -> Result<Vec<SearchHit>> {
    let query_vector = embed_query(query, semantic_backend)?;
    let vectors = db.get_search_vectors(semantic_backend, kind)?;
    if vectors.is_empty() {
        return Ok(base_hits);
    }
    let importance = graph.importance_scores();
    let max_importance = importance
        .values()
        .copied()
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let mut base_scores = HashMap::new();
    let max_base_score = base_hits
        .iter()
        .map(|hit| hit.score)
        .fold(0.0_f64, f64::max)
        .max(1.0);
    for hit in &base_hits {
        base_scores.insert(hit.node.id as i64, hit.score / max_base_score);
    }

    let mut merged = vectors
        .into_iter()
        .map(|candidate| {
            let vector_score = cosine_similarity(&query_vector, &candidate.vector).max(0.0) as f64;
            let lexical_score = base_scores.get(&candidate.node_id).copied().unwrap_or(0.0);
            let centrality = importance
                .get(&(candidate.node_id as u64))
                .copied()
                .unwrap_or(0.0)
                / max_importance;
            let kind_boost = if candidate.kind == "function" || candidate.kind == "method" {
                0.05
            } else if candidate.kind == "file" {
                0.0
            } else {
                0.02
            };
            let score =
                (lexical_score * 0.45) + (vector_score * 0.45) + (centrality * 0.10) + kind_boost;
            SearchHit {
                score,
                match_reason: if lexical_score > 0.0 {
                    "hybrid_fts_vector_graph".to_string()
                } else {
                    "semantic_vector_graph".to_string()
                },
                node: NodeSummary {
                    id: u64::try_from(candidate.node_id.max(0)).unwrap_or_default(),
                    name: candidate.name,
                    kind: candidate.kind,
                    file_path: Some(candidate.file_path),
                    line: None,
                    signature: candidate.signature,
                },
            }
        })
        .collect::<Vec<_>>();

    merged.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.node.name.cmp(&b.node.name))
    });
    merged.dedup_by(|a, b| a.node.id == b.node.id);
    if merged.is_empty() {
        return Ok(base_hits);
    }
    merged.truncate(limit);
    Ok(merged)
}

fn resolved_semantic_backend(db: &CacheDb, requested: Option<&str>) -> String {
    requested
        .map(str::to_string)
        .or_else(|| env::var("REPOSCRY_SEMANTIC_BACKEND").ok())
        .or_else(|| db.get_config("semantic_backend").ok().flatten())
        .unwrap_or_else(|| LOCAL_SEMANTIC_BACKEND.to_string())
}

fn embed_query(query: &str, backend: &str) -> Result<Vec<f32>> {
    match backend {
        OLLAMA_BACKEND => ollama_embedding(
            &env::var("REPOSCRY_OLLAMA_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:11434/api/embeddings".to_string()),
            &env::var("REPOSCRY_OLLAMA_MODEL").unwrap_or_else(|_| "nomic-embed-text".to_string()),
            query,
        ),
        _ => Ok(local_text_embedding(query)),
    }
}

fn local_text_embedding(text: &str) -> Vec<f32> {
    let mut vector = vec![0.0_f32; LOCAL_SEMANTIC_DIMS];
    for token in text
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .filter(|token| !token.is_empty())
    {
        let token = token.to_lowercase();
        let hash = blake3::hash(token.as_bytes());
        let bytes = hash.as_bytes();
        let bucket = usize::from(bytes[0]) % LOCAL_SEMANTIC_DIMS;
        let sign = if bytes[1] % 2 == 0 { 1.0 } else { -1.0 };
        let weight = 1.0 + (f32::from(bytes[2]) / 255.0);
        vector[bucket] += sign * weight;
    }
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in &mut vector {
            *value /= norm;
        }
    }
    vector
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.len() != right.len() || left.is_empty() {
        return 0.0;
    }
    let dot = left
        .iter()
        .zip(right.iter())
        .map(|(a, b)| a * b)
        .sum::<f32>();
    let left_norm = left.iter().map(|value| value * value).sum::<f32>().sqrt();
    let right_norm = right.iter().map(|value| value * value).sum::<f32>().sqrt();
    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        dot / (left_norm * right_norm)
    }
}

fn ollama_embedding(url: &str, model: &str, text: &str) -> Result<Vec<f32>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let response = client
        .post(url)
        .json(&json!({
            "model": model,
            "prompt": text,
        }))
        .send()?
        .error_for_status()?;
    let payload: Value = response.json()?;
    let embedding = payload
        .get("embedding")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("ollama response missing embedding array"))?;
    let vector = embedding
        .iter()
        .map(|value| value.as_f64().unwrap_or_default() as f32)
        .collect::<Vec<_>>();
    if vector.is_empty() {
        return Err(anyhow!("ollama returned an empty embedding"));
    }
    Ok(vector)
}

fn find_matching_nodes<'a>(graph: &'a CodeGraph, selector: &str) -> Vec<&'a GraphNode> {
    let selector_lower = selector.to_lowercase();
    let mut nodes = graph
        .nodes
        .values()
        .filter(|node| {
            node.name.to_lowercase().contains(&selector_lower)
                || node.file_path.as_deref().map_or(false, |p| {
                    p == selector || p.to_lowercase().contains(&selector_lower)
                })
        })
        .collect::<Vec<_>>();
    nodes.sort_by_key(|n| {
        (
            n.file_path.clone().unwrap_or_default(),
            n.start_line.unwrap_or(0),
        )
    });
    nodes
}

fn walk_reverse_imports(
    graph: &CodeGraph,
    start_paths: &[String],
    max_depth: usize,
) -> Vec<ImpactedFile> {
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
        if depth >= max_depth {
            continue;
        }
        for edge in graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Imports && e.target_id == node_id)
        {
            if let std::collections::hash_map::Entry::Vacant(entry) = visited.entry(edge.source_id)
            {
                entry.insert(depth + 1);
                queue.push_back((edge.source_id, depth + 1));
            }
        }
    }
    let mut files = visited
        .into_iter()
        .filter_map(|(id, depth)| {
            let path = graph.get_node(id)?.file_path.clone()?;
            let reason = if start_set.contains(&path) {
                "changed_or_target_file"
            } else {
                "reverse_import_dependency"
            };
            Some(ImpactedFile {
                path,
                depth,
                reason: reason.to_string(),
            })
        })
        .collect::<Vec<_>>();
    files.sort_by(|a, b| a.depth.cmp(&b.depth).then_with(|| a.path.cmp(&b.path)));
    files
}

fn infer_flows(
    graph: &CodeGraph,
    changed_paths: &[String],
    impacted: &[ImpactedFile],
) -> Vec<FlowSummary> {
    let impacted_paths = impacted
        .iter()
        .map(|file| file.path.clone())
        .collect::<HashSet<_>>();
    let changed_set = changed_paths.iter().cloned().collect::<HashSet<_>>();
    let changed_file_ids = changed_paths
        .iter()
        .filter_map(|path| file_node_by_path(graph, path))
        .map(|node| node.id)
        .collect::<HashSet<_>>();
    let changed_symbol_ids = changed_paths
        .iter()
        .filter_map(|path| file_node_by_path(graph, path))
        .flat_map(|file_node| {
            graph
                .edges
                .iter()
                .filter(move |edge| {
                    edge.kind == EdgeKind::Contains && edge.source_id == file_node.id
                })
                .map(|edge| edge.target_id)
                .collect::<Vec<_>>()
        })
        .collect::<HashSet<_>>();

    let mut flows = graph
        .nodes
        .values()
        .filter(|node| node.kind == NodeKind::File)
        .filter_map(|node| {
            let path = node.file_path.as_ref()?;
            let kind = classify_flow(path)?;
            let trace =
                trace_flow_from_entrypoint(graph, node.id, &changed_file_ids, &changed_symbol_ids)?;
            let changed_symbols = flow_changed_symbols(graph, &trace, &changed_set);
            let touched_files = trace
                .iter()
                .filter_map(|id| graph.get_node(*id))
                .filter_map(|node| node.file_path.clone())
                .filter(|path| changed_set.contains(path) || impacted_paths.contains(path))
                .collect::<HashSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let reason = flow_reason(graph, &trace, &changed_set);
            Some(FlowSummary {
                entrypoint: path.clone(),
                kind,
                path: render_flow_path(graph, &trace),
                changed_symbols,
                risk: flow_risk(graph, &trace, &touched_files),
                confidence: flow_confidence(&trace, &touched_files).to_string(),
                reason,
                touched_files,
            })
        })
        .collect::<Vec<_>>();

    flows.sort_by(|a, b| {
        b.path
            .len()
            .cmp(&a.path.len())
            .then_with(|| a.entrypoint.cmp(&b.entrypoint))
    });
    flows.dedup_by(|a, b| a.entrypoint == b.entrypoint && a.path == b.path);
    flows
}

fn trace_flow_from_entrypoint(
    graph: &CodeGraph,
    entrypoint_id: u64,
    changed_file_ids: &HashSet<u64>,
    changed_symbol_ids: &HashSet<u64>,
) -> Option<Vec<u64>> {
    let mut queue = VecDeque::from([(entrypoint_id, vec![entrypoint_id])]);
    let mut visited = HashSet::from([entrypoint_id]);

    while let Some((node_id, path)) = queue.pop_front() {
        if node_id != entrypoint_id
            && (changed_file_ids.contains(&node_id) || changed_symbol_ids.contains(&node_id))
        {
            return Some(path);
        }

        for next_id in flow_neighbors(graph, node_id) {
            if visited.insert(next_id) {
                let mut next_path = path.clone();
                next_path.push(next_id);
                queue.push_back((next_id, next_path));
            }
        }
    }

    None
}

fn flow_neighbors(graph: &CodeGraph, node_id: u64) -> Vec<u64> {
    let Some(node) = graph.get_node(node_id) else {
        return Vec::new();
    };
    let mut neighbors = Vec::new();

    match node.kind {
        NodeKind::File => {
            neighbors.extend(
                graph
                    .edges
                    .iter()
                    .filter(|edge| edge.kind == EdgeKind::Contains && edge.source_id == node_id)
                    .map(|edge| edge.target_id),
            );
            neighbors.extend(
                graph
                    .edges
                    .iter()
                    .filter(|edge| edge.kind == EdgeKind::Imports && edge.source_id == node_id)
                    .map(|edge| edge.target_id),
            );
        }
        _ => {
            neighbors.extend(
                graph
                    .edges
                    .iter()
                    .filter(|edge| edge.kind == EdgeKind::Calls && edge.source_id == node_id)
                    .map(|edge| edge.target_id),
            );
            if let Some(file_path) = node.file_path.as_deref() {
                if let Some(file_node) = file_node_by_path(graph, file_path) {
                    neighbors.extend(
                        graph
                            .edges
                            .iter()
                            .filter(|edge| {
                                edge.kind == EdgeKind::Imports && edge.source_id == file_node.id
                            })
                            .map(|edge| edge.target_id),
                    );
                }
            }
        }
    }

    neighbors
}

fn render_flow_path(graph: &CodeGraph, trace: &[u64]) -> Vec<String> {
    trace
        .iter()
        .filter_map(|id| graph.get_node(*id))
        .map(|node| match node.kind {
            NodeKind::File => node.file_path.clone().unwrap_or_else(|| node.name.clone()),
            _ => node.name.clone(),
        })
        .collect()
}

fn flow_changed_symbols(
    graph: &CodeGraph,
    trace: &[u64],
    changed_paths: &HashSet<String>,
) -> Vec<String> {
    let mut symbols = trace
        .iter()
        .filter_map(|id| graph.get_node(*id))
        .filter(|node| node.kind != NodeKind::File)
        .filter(|node| {
            node.file_path
                .as_ref()
                .map(|path| changed_paths.contains(path))
                .unwrap_or(false)
        })
        .map(|node| node.name.clone())
        .collect::<Vec<_>>();
    symbols.sort();
    symbols.dedup();
    symbols
}

fn flow_reason(graph: &CodeGraph, trace: &[u64], changed_paths: &HashSet<String>) -> String {
    if trace.len() <= 1 {
        return "entrypoint changed directly".to_string();
    }
    let ends_in_changed_symbol = trace
        .last()
        .and_then(|id| graph.get_node(*id))
        .map(|node| {
            node.kind != NodeKind::File
                && node
                    .file_path
                    .as_ref()
                    .map(|path| changed_paths.contains(path))
                    .unwrap_or(false)
        })
        .unwrap_or(false);
    if ends_in_changed_symbol {
        "entrypoint reaches changed symbol through indexed calls".to_string()
    } else {
        "entrypoint reaches changed file through indexed imports".to_string()
    }
}

fn flow_risk(graph: &CodeGraph, trace: &[u64], touched_files: &[String]) -> String {
    let touches_risky_path = touched_files.iter().any(|path| is_risky_path(path));
    let has_call_chain = trace.windows(2).any(|pair| {
        graph.edges.iter().any(|edge| {
            edge.kind == EdgeKind::Calls && edge.source_id == pair[0] && edge.target_id == pair[1]
        })
    });
    if touches_risky_path || has_call_chain || trace.len() >= 5 {
        "high".to_string()
    } else if trace.len() >= 3 {
        "medium".to_string()
    } else {
        "low".to_string()
    }
}

fn flow_confidence(trace: &[u64], touched_files: &[String]) -> &'static str {
    if trace.len() >= 4 && !touched_files.is_empty() {
        "high"
    } else if trace.len() >= 2 {
        "medium"
    } else {
        "low"
    }
}

fn classify_flow(path: &str) -> Option<String> {
    let lower = path.to_lowercase();
    if lower.ends_with("/middleware.ts") || lower.ends_with("/middleware.js") {
        Some("nextjs_middleware".to_string())
    } else if lower.ends_with("/route.ts")
        || lower.ends_with("/route.tsx")
        || lower.ends_with("/route.js")
        || lower.contains("/pages/api/")
    {
        Some("nextjs_api_route".to_string())
    } else if lower.ends_with("/layout.tsx")
        || lower.ends_with("/layout.ts")
        || lower.ends_with("/layout.jsx")
    {
        Some("nextjs_layout".to_string())
    } else if lower.ends_with("/page.tsx")
        || lower.ends_with("/page.ts")
        || lower.ends_with("/page.jsx")
    {
        Some("nextjs_page".to_string())
    } else if lower.contains("action")
        && (lower.ends_with(".ts") || lower.ends_with(".tsx") || lower.ends_with(".js"))
    {
        Some("server_action".to_string())
    } else if lower == "src/main.rs"
        || lower.ends_with("/src/main.rs")
        || lower.contains("/src/bin/")
    {
        Some("rust_binary".to_string())
    } else if lower.contains("axum") || lower.contains("actix") || lower.contains("/routes/") {
        Some("rust_route_module".to_string())
    } else if lower.ends_with("fastapi.py")
        || lower.contains("fastapi")
        || lower.contains("/views.py")
        || lower.contains("/django/")
    {
        Some("python_route".to_string())
    } else if lower.contains("celery") || lower.contains("task") && lower.ends_with(".py") {
        Some("python_task".to_string())
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
    impacted: &[ImpactedFile],
    flows: &[FlowSummary],
    tests: &[SuggestedTest],
) -> Vec<String> {
    let mut reasons = Vec::new();
    if impacted.len() > 20 {
        reasons.push(format!(
            "Wide blast radius: {} indexed files are affected.",
            impacted.len()
        ));
    }
    if !flows.is_empty() {
        reasons.push(format!(
            "{} entrypoint-like flow(s) are affected.",
            flows.len()
        ));
    }
    if tests.is_empty() && changes.iter().any(|c| !is_test_file(&c.path)) {
        reasons.push("Source files changed but no indexed test candidates were found.".to_string());
    }
    for change in changes {
        let fan_in_count = fan_in(graph, &change.path);
        if fan_in_count > 5 {
            reasons.push(format!(
                "{} has high fan-in: {} dependents.",
                change.path, fan_in_count
            ));
        }
        if is_risky_path(&change.path) {
            reasons.push(format!(
                "{} touches a high-risk path or concern.",
                change.path
            ));
        }
    }
    reasons.sort();
    reasons.dedup();
    reasons
}

fn risk_score(
    changes: &[GitChange],
    impacted: &[ImpactedFile],
    flows: &[FlowSummary],
    reasons: &[String],
) -> u32 {
    let changed_lines = changes
        .iter()
        .map(|c| c.lines_added + c.lines_deleted)
        .sum::<i64>();
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

fn suggested_tests_for_selector(
    graph: &CodeGraph,
    selector: &str,
    limit: usize,
) -> Vec<SuggestedTest> {
    let matched_nodes = find_matching_nodes(graph, selector);
    let paths = matched_nodes
        .iter()
        .filter_map(|node| node.file_path.clone())
        .collect::<Vec<_>>();
    suggested_tests(graph, &paths, &matched_nodes, limit)
}

fn suggested_tests_for_paths(
    graph: &CodeGraph,
    paths: &[String],
    limit: usize,
) -> Vec<SuggestedTest> {
    let matched_nodes = paths
        .iter()
        .filter_map(|path| file_node_by_path(graph, path))
        .collect::<Vec<_>>();
    suggested_tests(graph, paths, &matched_nodes, limit)
}

fn suggested_tests(
    graph: &CodeGraph,
    paths: &[String],
    matched_nodes: &[&GraphNode],
    limit: usize,
) -> Vec<SuggestedTest> {
    let mut relevant_paths = paths.iter().cloned().collect::<HashSet<_>>();
    let mut relevant_symbol_ids = HashSet::new();
    let mut terms = paths
        .iter()
        .flat_map(|path| search_terms(path))
        .filter(|term| term.len() > 2)
        .collect::<HashSet<_>>();

    for node in matched_nodes {
        terms.extend(search_terms(&node.name));
        if let Some(path) = node.file_path.clone() {
            relevant_paths.insert(path);
        }
        if node.kind != NodeKind::File && node.kind != NodeKind::Test {
            relevant_symbol_ids.insert(node.id);
        }
    }

    for path in &relevant_paths {
        if let Some(file_node) = file_node_by_path(graph, path) {
            for edge in graph
                .edges
                .iter()
                .filter(|edge| edge.kind == EdgeKind::Contains && edge.source_id == file_node.id)
            {
                if let Some(node) = graph.get_node(edge.target_id) {
                    if node.kind != NodeKind::Test {
                        relevant_symbol_ids.insert(node.id);
                        terms.extend(search_terms(&node.name));
                    }
                }
            }
        }
    }

    let target_file_ids = relevant_paths
        .iter()
        .filter_map(|path| file_node_by_path(graph, path))
        .map(|node| node.id)
        .collect::<HashSet<_>>();

    let mut candidates = graph
        .nodes
        .values()
        .filter(|node| node.kind == NodeKind::File)
        .filter_map(|node| {
            let path = node.file_path.clone()?;
            let has_test_symbols = graph.edges.iter().any(|edge| {
                edge.kind == EdgeKind::Contains
                    && edge.source_id == node.id
                    && graph
                        .get_node(edge.target_id)
                        .map(|child| child.kind == NodeKind::Test)
                        .unwrap_or(false)
            });
            (is_test_file(&path) || has_test_symbols).then_some((node.id, path))
        })
        .filter_map(|(test_file_id, path)| {
            let mut score = 0.0;
            let mut reasons = Vec::new();
            let mut matched_symbols = BTreeMap::new();
            let test_symbols = graph
                .edges
                .iter()
                .filter(|edge| edge.kind == EdgeKind::Contains && edge.source_id == test_file_id)
                .filter_map(|edge| graph.get_node(edge.target_id))
                .filter(|node| node.kind == NodeKind::Test)
                .collect::<Vec<_>>();

            let imported_targets = graph
                .edges
                .iter()
                .filter(|edge| edge.kind == EdgeKind::Imports && edge.source_id == test_file_id)
                .map(|edge| edge.target_id)
                .collect::<HashSet<_>>();
            let direct_imports = target_file_ids
                .iter()
                .filter(|target_id| imported_targets.contains(target_id))
                .count();
            if direct_imports > 0 {
                score += 5.0 + (direct_imports.saturating_sub(1) as f64);
                reasons.push(format!(
                    "directly imports {} changed/target file(s)",
                    direct_imports
                ));
            }

            let call_matches = graph
                .edges
                .iter()
                .filter(|edge| {
                    edge.kind == EdgeKind::Calls && relevant_symbol_ids.contains(&edge.target_id)
                })
                .filter_map(|edge| {
                    let source = graph.get_node(edge.source_id)?;
                    (source.kind == NodeKind::Test && source.file_path.as_deref() == Some(&path))
                        .then(|| (source.name.clone(), edge.target_id))
                })
                .collect::<Vec<_>>();
            if !call_matches.is_empty() {
                for (test_name, target_id) in &call_matches {
                    matched_symbols.insert(test_name.clone(), *target_id);
                }
                score += 6.0 + (call_matches.len().min(3) as f64);
                reasons.push(format!(
                    "test symbol call edges reach {} target symbol(s)",
                    call_matches.len()
                ));
            }

            let path_lower = path.to_lowercase();
            let name_overlap = terms
                .iter()
                .filter(|term| path_lower.contains(term.as_str()))
                .count();
            if name_overlap > 0 {
                score += (name_overlap as f64) * 1.5;
                reasons.push(format!("path/name overlap on {} term(s)", name_overlap));
            }

            if !paths.is_empty() {
                let target_modules = relevant_paths
                    .iter()
                    .map(|candidate| module_name(candidate))
                    .collect::<HashSet<_>>();
                if target_modules.contains(&module_name(&path)) {
                    score += 1.5;
                    reasons.push("same top-level module as target".to_string());
                }
            }

            if score <= 0.0 {
                return None;
            }

            let selected_symbols = if matched_symbols.is_empty() {
                test_symbols
                    .iter()
                    .map(|node| node.name.clone())
                    .take(2)
                    .collect::<Vec<_>>()
            } else {
                matched_symbols.keys().cloned().take(3).collect::<Vec<_>>()
            };

            Some(SuggestedTest {
                command: suggested_test_command(
                    &path,
                    selected_symbols.first().map(String::as_str),
                ),
                confidence: test_confidence(score).to_string(),
                score,
                reasons,
                test_symbols: selected_symbols,
                path,
            })
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });
    candidates.dedup_by(|a, b| a.path == b.path);
    candidates.truncate(limit);
    candidates
}

fn suggested_test_command(path: &str, test_symbol: Option<&str>) -> String {
    let normalized = path.replace('\\', "/");
    if normalized.ends_with(".rs") {
        if let Some(symbol) = test_symbol {
            return format!("cargo test {}", symbol);
        }
        return format!("cargo test {}", file_stem(&normalized));
    }
    if normalized.ends_with(".py") {
        if let Some(symbol) = test_symbol {
            return format!("pytest {}::{}", normalized, symbol);
        }
        return format!("pytest {}", normalized);
    }
    if normalized.ends_with(".ts")
        || normalized.ends_with(".tsx")
        || normalized.ends_with(".js")
        || normalized.ends_with(".jsx")
    {
        return format!("pnpm vitest {}", normalized);
    }
    format!("run tests covering {}", normalized)
}

fn file_stem(path: &str) -> &str {
    Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(path)
}

fn test_confidence(score: f64) -> &'static str {
    if score >= 10.0 {
        "high"
    } else if score >= 5.0 {
        "medium"
    } else {
        "low"
    }
}

fn top_degrees(graph: &CodeGraph, direction: Direction, limit: usize) -> Vec<FileDegree> {
    let mut degrees = graph
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::File)
        .filter_map(|n| {
            let path = n.file_path.clone()?;
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

fn summarize_modules(files: &[CachedFile]) -> Vec<ModuleSummary> {
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
    if parts.len() >= 2 && parts[0] == "crates" {
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
                .filter(|e| e.kind == EdgeKind::Imports && e.target_id == node.id)
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
                .filter(|e| e.kind == EdgeKind::Imports && e.source_id == node.id)
                .count()
        })
        .unwrap_or(0)
}

fn file_node_by_path<'a>(graph: &'a CodeGraph, path: &str) -> Option<&'a GraphNode> {
    graph
        .nodes
        .values()
        .find(|n| n.kind == NodeKind::File && n.file_path.as_deref() == Some(path))
}

fn display_node(node: &GraphNode) -> String {
    node.file_path.clone().unwrap_or_else(|| node.name.clone())
}

fn search_terms(input: &str) -> Vec<String> {
    input
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|p| p.len() > 2)
        .map(|p| p.to_lowercase())
        .collect()
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
        "auth",
        "session",
        "payment",
        "stripe",
        "billing",
        "db",
        "database",
        "schema",
        "migration",
        "security",
        "permission",
        "route",
        "api",
        "env",
        "config",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn graph_limitations() -> Vec<String> {
    vec![
        "Compatibility output uses indexed files, symbols, imports, persisted call sites, and persisted call edges.".to_string(),
        "Dynamic call resolution still falls back to heuristics when imports or symbol names are ambiguous.".to_string(),
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
    format!(
        "# Change risk analysis\n\n```json\n{}\n```",
        serde_json::to_string_pretty(value).unwrap_or_default()
    )
}

fn render_arch_markdown(value: &serde_json::Value) -> String {
    format!(
        "# Architecture overview\n\n```json\n{}\n```",
        serde_json::to_string_pretty(value).unwrap_or_default()
    )
}

fn render_simple_markdown(title: &str, value: &serde_json::Value) -> String {
    format!(
        "# {}\n\n```json\n{}\n```",
        title,
        serde_json::to_string_pretty(value).unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn file_node(graph: &mut CodeGraph, path: &str) -> u64 {
        let id = graph.add_node(NodeKind::File, path);
        let node = graph.nodes.get_mut(&id).unwrap();
        node.file_path = Some(path.to_string());
        id
    }

    fn symbol_node(graph: &mut CodeGraph, kind: NodeKind, file_path: &str, name: &str) -> u64 {
        let id = graph.add_node(kind, name);
        let node = graph.nodes.get_mut(&id).unwrap();
        node.file_path = Some(file_path.to_string());
        id
    }

    fn temp_repo_dir() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("reposcry-crg-test-{}", unique));
        fs::create_dir_all(path.join(CACHE_DIR)).unwrap();
        path
    }

    #[test]
    fn ranks_tests_using_direct_imports() {
        let mut graph = CodeGraph::new();
        let src = file_node(&mut graph, "src/orders.rs");
        let test = file_node(&mut graph, "tests/orders_spec.rs");
        graph.add_edge(test, src, EdgeKind::Imports);

        let suggestions = suggested_tests_for_paths(&graph, &["src/orders.rs".to_string()], 10);

        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].path, "tests/orders_spec.rs");
        assert!(suggestions[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("directly imports")));
        assert_eq!(suggestions[0].confidence, "medium");
    }

    #[test]
    fn ranks_tests_using_symbol_call_edges() {
        let mut graph = CodeGraph::new();
        let src = file_node(&mut graph, "src/cache.rs");
        let test_file = file_node(&mut graph, "tests/cache_test.rs");
        let target_symbol = symbol_node(
            &mut graph,
            NodeKind::Function,
            "src/cache.rs",
            "rebuild_graph",
        );
        let test_symbol = symbol_node(
            &mut graph,
            NodeKind::Test,
            "tests/cache_test.rs",
            "test_rebuild_graph",
        );
        graph.add_edge(src, target_symbol, EdgeKind::Contains);
        graph.add_edge(test_file, test_symbol, EdgeKind::Contains);
        graph.add_edge(test_symbol, target_symbol, EdgeKind::Calls);

        let suggestions = suggested_tests_for_selector(&graph, "rebuild_graph", 10);

        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].path, "tests/cache_test.rs");
        assert!(suggestions[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("call edges")));
        assert!(suggestions[0]
            .test_symbols
            .iter()
            .any(|name| name == "test_rebuild_graph"));
        assert_eq!(suggestions[0].confidence, "high");
    }

    #[test]
    fn infers_entrypoint_to_changed_symbol_flow() {
        let mut graph = CodeGraph::new();
        let entry_file = file_node(&mut graph, "src/main.rs");
        let changed_file = file_node(&mut graph, "src/orders.rs");
        let main_symbol = symbol_node(&mut graph, NodeKind::Function, "src/main.rs", "main");
        let changed_symbol = symbol_node(
            &mut graph,
            NodeKind::Function,
            "src/orders.rs",
            "process_order",
        );

        graph.add_edge(entry_file, main_symbol, EdgeKind::Contains);
        graph.add_edge(changed_file, changed_symbol, EdgeKind::Contains);
        graph.add_edge(main_symbol, changed_symbol, EdgeKind::Calls);

        let flows = infer_flows(&graph, &["src/orders.rs".to_string()], &[]);

        assert_eq!(flows.len(), 1);
        assert_eq!(flows[0].entrypoint, "src/main.rs");
        assert_eq!(flows[0].kind, "rust_binary");
        assert_eq!(flows[0].path, vec!["src/main.rs", "main", "process_order"]);
        assert_eq!(flows[0].changed_symbols, vec!["process_order"]);
        assert_eq!(flows[0].confidence, "medium");
        assert_eq!(flows[0].risk, "high");
    }

    #[test]
    fn mcp_initialize_returns_server_info() {
        let repo = temp_repo_dir();
        let db_path = repo.join(CACHE_DIR).join(CACHE_DB);
        let response = process_mcp_line(
            &repo,
            &db_path,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
            1024,
        )
        .unwrap();
        assert_eq!(response["result"]["serverInfo"]["name"], "reposcry");
        assert_eq!(response["result"]["protocolVersion"], "2024-11-05");
    }

    #[test]
    fn mcp_tools_list_returns_crg_tools() {
        let repo = temp_repo_dir();
        let db_path = repo.join(CACHE_DIR).join(CACHE_DB);
        let response = process_mcp_line(
            &repo,
            &db_path,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
            1024,
        )
        .unwrap();
        let tools = response["result"]["tools"].as_array().unwrap();
        assert!(tools.iter().any(|tool| tool["name"] == "detect_changes"));
        assert!(tools.iter().any(|tool| tool["name"] == "refactor_tool"));
    }

    #[test]
    fn mcp_tools_call_returns_content_block() {
        let repo = temp_repo_dir();
        let db_path = repo.join(CACHE_DIR).join(CACHE_DB);
        let response = process_mcp_line(
            &repo,
            &db_path,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_architecture_overview","arguments":{}}}"#,
            4096,
        )
        .unwrap();
        assert_eq!(response["result"]["isError"], false);
        let content = response["result"]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "text");
        assert!(content[0]["text"]
            .as_str()
            .unwrap()
            .contains("get_architecture_overview"));
    }

    #[test]
    fn mcp_invalid_json_returns_parse_error() {
        let repo = temp_repo_dir();
        let db_path = repo.join(CACHE_DIR).join(CACHE_DB);
        let response = process_mcp_line(&repo, &db_path, "{not-json", 1024).unwrap();
        assert_eq!(response["error"]["code"], -32700);
    }

    #[test]
    fn mcp_unknown_tool_returns_error() {
        let repo = temp_repo_dir();
        let db_path = repo.join(CACHE_DIR).join(CACHE_DB);
        let response = process_mcp_line(
            &repo,
            &db_path,
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"unknown_tool","arguments":{}}}"#,
            1024,
        )
        .unwrap();
        assert_eq!(response["error"]["code"], -32001);
    }

    #[test]
    fn hybrid_search_ranks_semantic_vector_hits() {
        let db = CacheDb::open_in_memory().unwrap();
        let mut graph = CodeGraph::new();
        let node_id = graph.add_node(NodeKind::Function, "cache_rebuild");
        let node = graph.nodes.get_mut(&node_id).unwrap();
        node.file_path = Some("src/cache.rs".to_string());
        node.signature = Some("fn cache_rebuild()".to_string());

        let vector = local_text_embedding("cache database calls");
        db.insert_search_vector(
            node_id as i64,
            "src/cache.rs",
            "function",
            "cache_rebuild",
            Some("fn cache_rebuild()"),
            LOCAL_SEMANTIC_BACKEND,
            &vector,
        )
        .unwrap();

        let hits = hybrid_search_nodes(
            &db,
            &graph,
            "cache database calls",
            None,
            10,
            Vec::new(),
            LOCAL_SEMANTIC_BACKEND,
        )
        .unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].node.name, "cache_rebuild");
        assert_eq!(hits[0].match_reason, "semantic_vector_graph");
    }

    #[test]
    fn hybrid_search_falls_back_to_lexical_hits_when_vectors_missing() {
        let db = CacheDb::open_in_memory().unwrap();
        let graph = CodeGraph::new();
        let base_hits = vec![SearchHit {
            score: 5.0,
            match_reason: "fts5".to_string(),
            node: NodeSummary {
                id: 1,
                name: "fallback".to_string(),
                kind: "file".to_string(),
                file_path: Some("README.md".to_string()),
                line: None,
                signature: None,
            },
        }];

        let hits = hybrid_search_nodes(
            &db,
            &graph,
            "cache database calls",
            None,
            10,
            base_hits.clone(),
            LOCAL_SEMANTIC_BACKEND,
        )
        .unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].node.name, "fallback");
        assert_eq!(hits[0].match_reason, "fts5");
    }
}
