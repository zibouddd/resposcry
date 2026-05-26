use std::collections::HashSet;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use clap::Parser;
use reposcry_cache::db::CacheDb;
use reposcry_graph::edge::EdgeKind;
use serde_json::{json, Value};

const CACHE_DIR: &str = ".reposcry";
const CACHE_DB: &str = "reposcry.db";

#[derive(Parser)]
#[command(name = "reposcry-mcp-plus", version, about = "Expanded RepoScry MCP server")]
struct Cli {
    #[arg(long = "repo", short = 'C', default_value = ".")]
    repo_root: String,

    #[arg(long, default_value_t = 1_048_576)]
    max_request_bytes: usize,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo_root = PathBuf::from(&cli.repo_root).canonicalize()?;
    let db_path = repo_root.join(CACHE_DIR).join(CACHE_DB);
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = handle_line(&repo_root, &db_path, &line, cli.max_request_bytes);
        if let Some(response) = response {
            writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
            stdout.flush()?;
        }
    }
    Ok(())
}

fn handle_line(repo_root: &Path, db_path: &Path, line: &str, max_request_bytes: usize) -> Option<Value> {
    if line.len() > max_request_bytes {
        return Some(error(None, -32000, "request too large", json!({"max_request_bytes": max_request_bytes})));
    }

    let request: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(err) => return Some(error(None, -32700, "parse error", json!({"details": err.to_string()}))),
    };
    let id = request.get("id").cloned();
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let params = request.get("params").cloned().unwrap_or_else(|| json!({}));

    match method {
        "initialize" => Some(success(id, json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {"name": "reposcry-mcp-plus", "version": env!("CARGO_PKG_VERSION")}
        }))),
        "notifications/initialized" => None,
        "tools/list" => Some(success(id, json!({"tools": tool_specs()}))),
        "tools/call" => match call_tool(repo_root, db_path, &params) {
            Ok(value) => Some(success(id, json!({
                "content": [{"type": "text", "text": serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())}],
                "isError": false
            }))),
            Err(err) => Some(error(id, -32001, "tool call failed", json!({"details": err.to_string()}))),
        },
        other => Some(error(id, -32601, "method not found", json!({"method": other}))),
    }
}

fn success(id: Option<Value>, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id.unwrap_or(Value::Null), "result": result})
}

fn error(id: Option<Value>, code: i64, message: &str, data: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id.unwrap_or(Value::Null), "error": {"code": code, "message": message, "data": data}})
}

fn tool_specs() -> Vec<Value> {
    vec![
        spec("get_graph_summary", "Return graph counts and language totals", json!({"type":"object","properties":{}})),
        spec("list_languages", "List indexed languages and file counts", json!({"type":"object","properties":{}})),
        spec("list_files", "List indexed files", json!({"type":"object","properties":{"language":{"type":"string"},"prefix":{"type":"string"},"limit":{"type":"integer"}}})),
        spec("list_symbols", "List indexed symbols", json!({"type":"object","properties":{"file":{"type":"string"},"kind":{"type":"string"},"limit":{"type":"integer"}}})),
        spec("get_file_neighborhood", "Return imports, importers, callers, and callees for a file", json!({"type":"object","properties":{"file":{"type":"string"}},"required":["file"]})),
        spec("export_graph_json", "Return a compact file-level graph", json!({"type":"object","properties":{"limit":{"type":"integer"}}})),
    ]
}

fn spec(name: &str, description: &str, input_schema: Value) -> Value {
    json!({"name": name, "description": description, "inputSchema": input_schema})
}

fn call_tool(repo_root: &Path, db_path: &Path, params: &Value) -> Result<Value> {
    let name = params.get("name").and_then(Value::as_str).ok_or_else(|| anyhow!("missing tool name"))?;
    let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    let db = CacheDb::open(db_path)?;

    match name {
        "get_graph_summary" => Ok(json!({
            "tool": name,
            "repo": repo_root.display().to_string(),
            "files": db.file_count()?,
            "symbols": db.symbol_count()?,
            "imports": db.import_count()?,
            "call_sites": db.call_site_count()?,
            "file_edges": db.edge_count()?,
            "symbol_call_edges": db.symbol_edge_count()?,
            "languages": db.language_stats()?,
        })),
        "list_languages" => Ok(json!({"tool": name, "languages": db.language_stats()?})),
        "list_files" => list_files(&db, &args),
        "list_symbols" => list_symbols(&db, &args),
        "get_file_neighborhood" => file_neighborhood(&db, required_string(&args, "file")?),
        "export_graph_json" => export_graph_json(&db, optional_usize(&args, "limit").unwrap_or(2000)),
        other => Err(anyhow!("unknown tool: {}", other)),
    }
}

fn list_files(db: &CacheDb, args: &Value) -> Result<Value> {
    let language = args.get("language").and_then(Value::as_str);
    let prefix = args.get("prefix").and_then(Value::as_str);
    let limit = optional_usize(args, "limit").unwrap_or(200);
    let mut files = db.get_all_files()?;
    files.retain(|file| language.map(|lang| file.language == lang).unwrap_or(true));
    files.retain(|file| prefix.map(|value| file.path.starts_with(value)).unwrap_or(true));
    files.truncate(limit);
    Ok(json!({"tool": "list_files", "files": files}))
}

fn list_symbols(db: &CacheDb, args: &Value) -> Result<Value> {
    let file_filter = args.get("file").and_then(Value::as_str);
    let kind_filter = args.get("kind").and_then(Value::as_str);
    let limit = optional_usize(args, "limit").unwrap_or(300);
    let mut symbols = Vec::new();

    for file in db.get_all_files()? {
        if file_filter.map(|path| path != file.path).unwrap_or(false) {
            continue;
        }
        for symbol in db.get_symbols_by_file(file.id)? {
            if kind_filter.map(|kind| kind != symbol.kind).unwrap_or(false) {
                continue;
            }
            symbols.push(json!({
                "file": file.path.clone(),
                "name": symbol.name,
                "kind": symbol.kind,
                "start_line": symbol.start_line,
                "end_line": symbol.end_line,
                "signature": symbol.signature,
            }));
            if symbols.len() >= limit {
                return Ok(json!({"tool": "list_symbols", "symbols": symbols}));
            }
        }
    }
    Ok(json!({"tool": "list_symbols", "symbols": symbols}))
}

fn file_neighborhood(db: &CacheDb, file_path: &str) -> Result<Value> {
    let files = db.get_all_files()?;
    let file = files.iter().find(|file| file.path == file_path).ok_or_else(|| anyhow!("indexed file not found: {}", file_path))?;
    let mut imported_files = Vec::new();
    let mut importers = Vec::new();
    let mut callees = Vec::new();
    let mut callers = Vec::new();

    for edge in db.get_edges_by_kind(EdgeKind::Imports)? {
        if edge.source_file_id == file.id {
            if let Some(path) = edge.target_path {
                imported_files.push(path);
            }
        } else if edge.target_file_id == Some(file.id) {
            if let Some(source) = files.iter().find(|candidate| candidate.id == edge.source_file_id) {
                importers.push(source.path.clone());
            }
        }
    }

    for edge in db.get_edges_by_kind(EdgeKind::Calls)? {
        if edge.source_file_id == file.id {
            if let Some(target_id) = edge.target_file_id {
                if let Some(target) = files.iter().find(|candidate| candidate.id == target_id) {
                    callees.push(target.path.clone());
                }
            }
        } else if edge.target_file_id == Some(file.id) {
            if let Some(source) = files.iter().find(|candidate| candidate.id == edge.source_file_id) {
                callers.push(source.path.clone());
            }
        }
    }

    sort_unique(&mut imported_files);
    sort_unique(&mut importers);
    sort_unique(&mut callees);
    sort_unique(&mut callers);

    Ok(json!({
        "tool": "get_file_neighborhood",
        "file": file_path,
        "imports": db.get_imports_by_file(file.id)?,
        "imported_files": imported_files,
        "importers": importers,
        "callees": callees,
        "callers": callers,
    }))
}

fn export_graph_json(db: &CacheDb, limit: usize) -> Result<Value> {
    let files = db.get_all_files()?;
    let allowed = files.iter().take(limit).map(|file| file.id).collect::<HashSet<_>>();
    let nodes = files
        .iter()
        .take(limit)
        .map(|file| json!({"id": format!("file:{}", file.id), "label": file.path, "kind": "file", "language": file.language}))
        .collect::<Vec<_>>();
    let mut edges = Vec::new();

    for edge in db.get_edges_by_kind(EdgeKind::Imports)? {
        if allowed.contains(&edge.source_file_id) && edge.target_file_id.map(|id| allowed.contains(&id)).unwrap_or(false) {
            edges.push(json!({"source": format!("file:{}", edge.source_file_id), "target": format!("file:{}", edge.target_file_id.unwrap_or_default()), "kind": "imports"}));
        }
    }
    for edge in db.get_edges_by_kind(EdgeKind::Calls)? {
        if allowed.contains(&edge.source_file_id) && edge.target_file_id.map(|id| allowed.contains(&id)).unwrap_or(false) {
            edges.push(json!({"source": format!("file:{}", edge.source_file_id), "target": format!("file:{}", edge.target_file_id.unwrap_or_default()), "kind": "calls"}));
        }
    }

    Ok(json!({"tool": "export_graph_json", "nodes": nodes, "edges": edges}))
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value.get(key).and_then(Value::as_str).ok_or_else(|| anyhow!("missing required string argument: {}", key))
}

fn optional_usize(value: &Value, key: &str) -> Option<usize> {
    value.get(key).and_then(Value::as_u64).and_then(|value| usize::try_from(value).ok())
}

fn sort_unique(values: &mut Vec<String>) {
    values.sort();
    values.dedup();
}
