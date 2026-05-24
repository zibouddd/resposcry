use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Parser, ValueEnum};
use reposcry_cache::db::CacheDb;
use reposcry_graph::edge::EdgeKind;
use serde::Serialize;

const CACHE_DIR: &str = ".reposcry";
const CACHE_DB: &str = "reposcry.db";

#[derive(Parser)]
#[command(name = "reposcry-export", version, about = "Export the RepoScry graph as JSON, GraphML, or HTML")]
struct Cli {
    #[arg(long = "repo", short = 'C', default_value = ".")]
    repo_root: String,
    #[arg(long, value_enum, default_value_t = ExportFormat::Json)]
    format: ExportFormat,
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    symbols: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ExportFormat { Json, Graphml, Html }

#[derive(Debug, Serialize)]
struct ExportNode {
    id: String,
    label: String,
    kind: String,
    path: Option<String>,
    language: Option<String>,
    line: Option<u32>,
}

#[derive(Debug, Serialize)]
struct ExportEdge {
    source: String,
    target: String,
    kind: String,
    confidence: f64,
}

#[derive(Debug, Serialize)]
struct ExportGraph {
    tool: String,
    repo: String,
    nodes: Vec<ExportNode>,
    edges: Vec<ExportEdge>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo_root = PathBuf::from(&cli.repo_root).canonicalize()?;
    let db = CacheDb::open(&repo_root.join(CACHE_DIR).join(CACHE_DB))?;
    let graph = build_export_graph(&repo_root, &db, cli.symbols)?;
    let content = match cli.format {
        ExportFormat::Json => serde_json::to_string_pretty(&graph)?,
        ExportFormat::Graphml => render_graphml(&graph),
        ExportFormat::Html => render_html(&graph)?,
    };
    if let Some(output) = cli.output {
        if let Some(parent) = output.parent() {
            if !parent.as_os_str().is_empty() { fs::create_dir_all(parent)?; }
        }
        fs::write(output, content)?;
    } else {
        println!("{}", content);
    }
    Ok(())
}

fn build_export_graph(repo_root: &Path, db: &CacheDb, include_symbols: bool) -> Result<ExportGraph> {
    let files = db.get_all_files()?;
    let mut file_ids = BTreeMap::<i64, String>::new();
    let mut symbol_ids = BTreeMap::<i64, String>::new();
    let mut nodes = Vec::<ExportNode>::new();
    let mut edges = Vec::<ExportEdge>::new();

    for file in &files {
        let id = format!("file:{}", file.id);
        file_ids.insert(file.id, id.clone());
        nodes.push(ExportNode { id, label: file.path.clone(), kind: "file".to_string(), path: Some(file.path.clone()), language: Some(file.language.clone()), line: None });
    }

    if include_symbols {
        for file in &files {
            let Some(file_id) = file_ids.get(&file.id).cloned() else { continue; };
            for symbol in db.get_symbols_by_file(file.id)? {
                let Some(symbol_db_id) = symbol.id else { continue; };
                let id = format!("symbol:{}", symbol_db_id);
                symbol_ids.insert(symbol_db_id, id.clone());
                nodes.push(ExportNode { id: id.clone(), label: symbol.name.clone(), kind: symbol.kind.clone(), path: Some(file.path.clone()), language: Some(file.language.clone()), line: Some(symbol.start_line) });
                edges.push(ExportEdge { source: file_id.clone(), target: id, kind: "contains".to_string(), confidence: 1.0 });
            }
        }
    }

    for edge in db.get_edges_by_kind(EdgeKind::Imports)? {
        if let (Some(source), Some(target_file_id)) = (file_ids.get(&edge.source_file_id).cloned(), edge.target_file_id) {
            if let Some(target) = file_ids.get(&target_file_id).cloned() {
                edges.push(ExportEdge { source, target, kind: "imports".to_string(), confidence: edge.confidence });
            }
        }
    }
    for edge in db.get_edges_by_kind(EdgeKind::Calls)? {
        if let (Some(source), Some(target_file_id)) = (file_ids.get(&edge.source_file_id).cloned(), edge.target_file_id) {
            if let Some(target) = file_ids.get(&target_file_id).cloned() {
                edges.push(ExportEdge { source, target, kind: "calls".to_string(), confidence: edge.confidence });
            }
        }
    }
    if include_symbols {
        for edge in db.get_symbol_edges_by_kind(EdgeKind::Calls.as_str())? {
            if let (Some(source), Some(target)) = (symbol_ids.get(&edge.source_symbol_id).cloned(), symbol_ids.get(&edge.target_symbol_id).cloned()) {
                edges.push(ExportEdge { source, target, kind: "symbol_calls".to_string(), confidence: edge.confidence });
            }
        }
    }

    Ok(ExportGraph { tool: "reposcry-export".to_string(), repo: repo_root.display().to_string(), nodes, edges })
}

fn render_graphml(graph: &ExportGraph) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\">\n  <graph id=\"reposcry\" edgedefault=\"directed\">\n");
    for node in &graph.nodes {
        out.push_str(&format!("    <node id=\"{}\"><data key=\"label\">{}</data><data key=\"kind\">{}</data></node>\n", xml_escape(&node.id), xml_escape(&node.label), xml_escape(&node.kind)));
    }
    for (index, edge) in graph.edges.iter().enumerate() {
        out.push_str(&format!("    <edge id=\"e{}\" source=\"{}\" target=\"{}\"><data key=\"kind\">{}</data></edge>\n", index, xml_escape(&edge.source), xml_escape(&edge.target), xml_escape(&edge.kind)));
    }
    out.push_str("  </graph>\n</graphml>\n");
    out
}

fn render_html(graph: &ExportGraph) -> Result<String> {
    let mut rows = String::new();
    for node in graph.nodes.iter().take(1000) {
        rows.push_str(&format!("<tr><td>{}</td><td>{}</td><td>{}</td></tr>", html_escape(&node.kind), html_escape(&node.label), html_escape(node.path.as_deref().unwrap_or(""))));
    }
    Ok(format!("<!doctype html><html><head><meta charset=\"utf-8\"><title>RepoScry graph</title><style>body{{font-family:system-ui;background:#0c0c0e;color:#f4f4f5;padding:24px}}table{{width:100%;border-collapse:collapse}}td,th{{border-bottom:1px solid #333;padding:6px;text-align:left}}</style></head><body><h1>RepoScry graph</h1><p>{} nodes, {} edges</p><table><thead><tr><th>Kind</th><th>Label</th><th>Path</th></tr></thead><tbody>{}</tbody></table></body></html>", graph.nodes.len(), graph.edges.len(), rows))
}

fn xml_escape(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn html_escape(value: &str) -> String {
    xml_escape(value).replace('\'', "&#39;")
}
