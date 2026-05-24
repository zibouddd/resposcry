use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Parser, ValueEnum};
use reposcry_cache::db::{CacheDb, CachedFile};
use reposcry_graph::edge::EdgeKind;
use serde::Serialize;
use serde_json::json;

const CACHE_DIR: &str = ".reposcry";
const CACHE_DB: &str = "reposcry.db";

#[derive(Parser)]
#[command(
    name = "reposcry-export",
    version,
    about = "Export the RepoScry graph as JSON, GraphML, or HTML"
)]
struct Cli {
    #[arg(long = "repo", short = 'C', default_value = ".")]
    repo_root: String,

    #[arg(long, value_enum, default_value_t = ExportFormat::Json)]
    format: ExportFormat,

    #[arg(long)]
    output: Option<PathBuf>,

    /// Include symbol nodes and symbol call edges. File graph only by default.
    #[arg(long, default_value_t = false)]
    symbols: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ExportFormat {
    Json,
    Graphml,
    Html,
}

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
    let db_path = repo_root.join(CACHE_DIR).join(CACHE_DB);
    let db = CacheDb::open(&db_path)?;
    let graph = build_export_graph(&repo_root, &db, cli.symbols)?;
    let content = match cli.format {
        ExportFormat::Json => serde_json::to_string_pretty(&graph)?,
        ExportFormat::Graphml => render_graphml(&graph),
        ExportFormat::Html => render_html(&graph)?,
    };

    if let Some(output) = cli.output {
        if let Some(parent) = output.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(output, content)?;
    } else {
        println!("{}", content);
    }
    Ok(())
}

fn build_export_graph(repo_root: &Path, db: &CacheDb, include_symbols: bool) -> Result<ExportGraph> {
    let files = db.get_all_files()?;
    let mut file_id_to_node_id = BTreeMap::<i64, String>::new();
    let mut symbol_id_to_node_id = BTreeMap::<i64, String>::new();
    let mut nodes = Vec::<ExportNode>::new();
    let mut edges = Vec::<ExportEdge>::new();

    for file in &files {
        let node_id = file_node_id(file.id);
        file_id_to_node_id.insert(file.id, node_id.clone());
        nodes.push(ExportNode {
            id: node_id,
            label: file.path.clone(),
            kind: "file".to_string(),
            path: Some(file.path.clone()),
            language: Some(file.language.clone()),
            line: None,
        });
    }

    if include_symbols {
        for file in &files {
            let Some(file_node_id) = file_id_to_node_id.get(&file.id).cloned() else {
                continue;
            };
            for symbol in db.get_symbols_by_file(file.id)? {
                let Some(symbol_id) = symbol.id else {
                    continue;
                };
                let node_id = symbol_node_id(symbol_id);
                symbol_id_to_node_id.insert(symbol_id, node_id.clone());
                nodes.push(ExportNode {
                    id: node_id.clone(),
                    label: symbol.name.clone(),
                    kind: symbol.kind.clone(),
                    path: Some(file.path.clone()),
                    language: Some(file.language.clone()),
                    line: Some(symbol.start_line),
                });
                edges.push(ExportEdge {
                    source: file_node_id.clone(),
                    target: node_id,
                    kind: "contains".to_string(),
                    confidence: 1.0,
                });
            }
        }
    }

    for edge in db.get_edges_by_kind(EdgeKind::Imports)? {
        let Some(source) = file_id_to_node_id.get(&edge.source_file_id).cloned() else {
            continue;
        };
        let Some(target_file_id) = edge.target_file_id else {
            continue;
        };
        let Some(target) = file_id_to_node_id.get(&target_file_id).cloned() else {
            continue;
        };
        edges.push(ExportEdge {
            source,
            target,
            kind: "imports".to_string(),
            confidence: edge.confidence,
        });
    }

    for edge in db.get_edges_by_kind(EdgeKind::Calls)? {
        let Some(source) = file_id_to_node_id.get(&edge.source_file_id).cloned() else {
            continue;
        };
        let Some(target_file_id) = edge.target_file_id else {
            continue;
        };
        let Some(target) = file_id_to_node_id.get(&target_file_id).cloned() else {
            continue;
        };
        edges.push(ExportEdge {
            source,
            target,
            kind: "calls".to_string(),
            confidence: edge.confidence,
        });
    }

    if include_symbols {
        for edge in db.get_symbol_edges_by_kind(EdgeKind::Calls.as_str())? {
            let Some(source) = symbol_id_to_node_id.get(&edge.source_symbol_id).cloned() else {
                continue;
            };
            let Some(target) = symbol_id_to_node_id.get(&edge.target_symbol_id).cloned() else {
                continue;
            };
            edges.push(ExportEdge {
                source,
                target,
                kind: "symbol_calls".to_string(),
                confidence: edge.confidence,
            });
        }
    }

    Ok(ExportGraph {
        tool: "reposcry-export".to_string(),
        repo: repo_root.display().to_string(),
        nodes,
        edges,
    })
}

fn file_node_id(file_id: i64) -> String {
    format!("file:{}", file_id)
}

fn symbol_node_id(symbol_id: i64) -> String {
    format!("symbol:{}", symbol_id)
}

fn render_graphml(graph: &ExportGraph) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\">\n");
    out.push_str("  <key id=\"label\" for=\"node\" attr.name=\"label\" attr.type=\"string\"/>\n");
    out.push_str("  <key id=\"kind\" for=\"all\" attr.name=\"kind\" attr.type=\"string\"/>\n");
    out.push_str("  <key id=\"path\" for=\"node\" attr.name=\"path\" attr.type=\"string\"/>\n");
    out.push_str("  <key id=\"confidence\" for=\"edge\" attr.name=\"confidence\" attr.type=\"double\"/>\n");
    out.push_str("  <graph id=\"reposcry\" edgedefault=\"directed\">\n");
    for node in &graph.nodes {
        out.push_str(&format!("    <node id=\"{}\">\n", xml_escape(&node.id)));
        out.push_str(&format!("      <data key=\"label\">{}</data>\n", xml_escape(&node.label)));
        out.push_str(&format!("      <data key=\"kind\">{}</data>\n", xml_escape(&node.kind)));
        if let Some(path) = &node.path {
            out.push_str(&format!("      <data key=\"path\">{}</data>\n", xml_escape(path)));
        }
        out.push_str("    </node>\n");
    }
    for (index, edge) in graph.edges.iter().enumerate() {
        out.push_str(&format!(
            "    <edge id=\"e{}\" source=\"{}\" target=\"{}\">\n",
            index,
            xml_escape(&edge.source),
            xml_escape(&edge.target)
        ));
        out.push_str(&format!("      <data key=\"kind\">{}</data>\n", xml_escape(&edge.kind)));
        out.push_str(&format!("      <data key=\"confidence\">{}</data>\n", edge.confidence));
        out.push_str("    </edge>\n");
    }
    out.push_str("  </graph>\n</graphml>\n");
    out
}

fn render_html(graph: &ExportGraph) -> Result<String> {
    let graph_json = serde_json::to_string(&json!({
        "nodes": graph.nodes,
        "edges": graph.edges,
        "repo": graph.repo,
    }))?;
    Ok(format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>RepoScry graph export</title>
<style>
body {{ margin: 0; font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background: #0c0c0e; color: #f4f4f5; }}
main {{ padding: 24px; }}
.controls {{ display: flex; gap: 12px; align-items: center; margin-bottom: 16px; }}
input {{ background: #18181b; color: #f4f4f5; border: 1px solid #3f3f46; border-radius: 8px; padding: 8px 10px; width: 320px; }}
.grid {{ display: grid; grid-template-columns: 360px 1fr; gap: 16px; }}
.panel {{ background: #18181b; border: 1px solid #27272a; border-radius: 12px; padding: 12px; overflow: auto; max-height: 75vh; }}
.node {{ padding: 8px; border-bottom: 1px solid #27272a; cursor: pointer; }}
.node:hover {{ background: #27272a; }}
.muted {{ color: #a1a1aa; font-size: 12px; }}
canvas {{ width: 100%; height: 75vh; background: radial-gradient(circle at center, #18181b 0, #0c0c0e 70%); border: 1px solid #27272a; border-radius: 12px; }}
</style>
</head>
<body>
<main>
  <h1>RepoScry graph export</h1>
  <p class="muted" id="meta"></p>
  <div class="controls"><input id="search" placeholder="Filter nodes by path, label, kind" /></div>
  <div class="grid">
    <section class="panel" id="list"></section>
    <canvas id="graph" width="1200" height="800"></canvas>
  </div>
</main>
<script>
const data = {graph_json};
const meta = document.getElementById('meta');
const list = document.getElementById('list');
const search = document.getElementById('search');
const canvas = document.getElementById('graph');
const ctx = canvas.getContext('2d');
meta.textContent = `${{data.repo}} — ${{data.nodes.length}} nodes, ${{data.edges.length}} edges`;
function filteredNodes() {{
  const q = search.value.toLowerCase();
  return data.nodes.filter(n => !q || [n.id, n.label, n.kind, n.path || ''].join(' ').toLowerCase().includes(q)).slice(0, 500);
}}
function renderList(nodes) {{
  list.innerHTML = '';
  for (const node of nodes) {{
    const div = document.createElement('div');
    div.className = 'node';
    div.innerHTML = `<strong>${{escapeHtml(node.label)}}</strong><div class="muted">${{escapeHtml(node.kind)}} · ${{escapeHtml(node.path || '')}}</div>`;
    list.appendChild(div);
  }}
}}
function renderGraph(nodes) {{
  ctx.clearRect(0,0,canvas.width,canvas.height);
  const ids = new Set(nodes.map(n => n.id));
  const byId = new Map(nodes.map((n, i) => [n.id, {{...n, x: 80 + (i * 97) % 1040, y: 80 + (i * 53) % 640}}]));
  ctx.lineWidth = 1;
  ctx.strokeStyle = 'rgba(161,161,170,0.25)';
  for (const edge of data.edges) {{
    if (!ids.has(edge.source) || !ids.has(edge.target)) continue;
    const a = byId.get(edge.source), b = byId.get(edge.target);
    ctx.beginPath(); ctx.moveTo(a.x, a.y); ctx.lineTo(b.x, b.y); ctx.stroke();
  }}
  for (const node of byId.values()) {{
    ctx.beginPath(); ctx.arc(node.x, node.y, node.kind === 'file' ? 5 : 3, 0, Math.PI * 2); ctx.fillStyle = node.kind === 'file' ? '#f4f4f5' : '#a1a1aa'; ctx.fill();
  }}
}}
function escapeHtml(value) {{ return String(value).replace(/[&<>"']/g, c => ({{'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}}[c])); }}
function render() {{ const nodes = filteredNodes(); renderList(nodes); renderGraph(nodes); }}
search.addEventListener('input', render);
render();
</script>
</body>
</html>
"#
    ))
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
