use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use candle_core::{DType, Device};
use clap::{Parser, Subcommand};
use fastembed::{
    EmbeddingModel, InitOptions, NomicV2MoeTextEmbedding, Qwen3TextEmbedding, TextEmbedding,
};
use serde::Serialize;

#[allow(dead_code)]
#[path = "crg_cli.rs"]
mod crg_cli;
mod installer;

use installer::{install_platform, InstallOptions, InstallPlatform};
use tracing::{info, trace, warn};
use tracing_subscriber::EnvFilter;

use reposcry_cache::db::{CacheDb, CachedCallSite, CachedFile, CachedImport, CachedSymbolEdge};
use reposcry_context::{ContextBuilder, ContextConfig, OutputFormat};
use reposcry_git::GitIntegration;
use reposcry_graph::edge::EdgeKind;
use reposcry_graph::graph::CodeGraph;
use reposcry_graph::node::NodeKind;
use reposcry_indexer::parser::parse_file;
use reposcry_indexer::preset::get_preset;
use reposcry_indexer::scanner::FileScanner;
use reposcry_report::{self, generate_report, render_markdown};
use reposcry_rules::{RulesConfig, RulesEngine};

const CACHE_DIR: &str = ".reposcry";
const CACHE_DB: &str = "reposcry.db";
const IGNORE_FILE: &str = ".reposcryignore";
const LOCAL_SEMANTIC_BACKEND: &str = "local-hash-v1";
const LOCAL_SEMANTIC_DIMS: usize = 64;
const OLLAMA_BACKEND: &str = "ollama";
const FASTEMBED_BACKEND: &str = "fastembed";
const CANDLE_BACKEND: &str = "candle";
const GRAPH_CACHE_STATUS_KEY: &str = "graph_cache_status";
const SEARCH_INDEX_STATUS_KEY: &str = "search_index_status";

#[derive(Parser)]
#[command(name = "reposcry", version, about = "RepoScry — AI context engine")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(global = true, long = "repo", short = 'C', default_value = ".")]
    repo_root: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize the repository for RepoScry tracking
    Init,
    /// Install RepoScry instructions, skills, and hooks for an AI coding platform
    Install {
        /// Target platform
        #[arg(long, value_enum, default_value_t = InstallPlatform::Claude)]
        platform: InstallPlatform,
        /// Overwrite files that already exist
        #[arg(long)]
        force: bool,
        /// Show what would be installed without writing files
        #[arg(long)]
        dry_run: bool,
    },
    /// Install RepoScry integration for MCP/agent platforms (defaults to all)
    InstallMcp {
        /// Target platform
        #[arg(long, value_enum, default_value_t = InstallPlatform::All)]
        platform: InstallPlatform,
        /// Overwrite files that already exist
        #[arg(long)]
        force: bool,
        /// Show what would be installed without writing files
        #[arg(long)]
        dry_run: bool,
    },
    /// VS Code Copilot Chat installer: `reposcry vscode install`
    Vscode {
        #[command(subcommand)]
        action: InstallAction,
    },
    /// Cursor installer: `reposcry cursor install`
    Cursor {
        #[command(subcommand)]
        action: InstallAction,
    },
    /// Kiro installer: `reposcry kiro install`
    Kiro {
        #[command(subcommand)]
        action: InstallAction,
    },
    /// Google Antigravity installer: `reposcry antigravity install`
    Antigravity {
        #[command(subcommand)]
        action: InstallAction,
    },
    /// Local hook installer: `reposcry hooks install`
    Hooks {
        #[command(subcommand)]
        action: InstallAction,
    },
    /// Scan and index repository files
    Index {
        /// Preset to use (nextjs, rust, tauri, monorepo, python)
        #[arg(long)]
        preset: Option<String>,
        /// Skip semantic vector refresh and rebuild only lexical search documents
        #[arg(long, default_value_t = false)]
        no_semantic: bool,
        /// Override the semantic backend used for index-time vector refresh
        #[arg(long)]
        semantic_backend: Option<String>,
        /// Force a full vector rebuild for the selected backend instead of reusing cached vectors
        #[arg(long, default_value_t = false)]
        reembed_all: bool,
    },
    /// Show repository statistics
    Stats,
    /// List indexed files
    Files {
        /// Filter by language
        #[arg(long)]
        language: Option<String>,
    },
    /// Show symbols in a file
    Symbols { file: String },
    /// Show file dependencies
    Deps { file: String },
    /// Show reverse dependencies
    Rdeps { file: String },
    /// Show diff impact
    Diff {
        base: String,
        #[arg(default_value = "HEAD")]
        head: String,
    },
    /// Generate AI context pack (default: compact 4k budget)
    Context {
        task: String,
        /// Token budget (default: 4000 compact, 8000 deep, 20000 full)
        #[arg(long, default_value = "4000")]
        budget: u32,
        /// Strict mode
        #[arg(long)]
        strict: bool,
        /// Include full file contents
        #[arg(long)]
        full: bool,
        /// Output format (human, markdown, json)
        #[arg(long, default_value = "markdown")]
        format: String,
        /// Max symbols per file (default: 8)
        #[arg(long, default_value = "8")]
        max_symbols_per_file: u32,
        /// Show count of omitted symbols per file
        #[arg(long)]
        show_omitted: bool,
    },
    /// Generate PR review report
    Report {
        base: String,
        #[arg(default_value = "HEAD")]
        head: String,
        /// Output format (markdown, json)
        #[arg(long, default_value = "markdown")]
        format: String,
    },
    /// Check architecture rules
    Rules {
        #[command(subcommand)]
        action: Option<RulesAction>,
    },
    /// Validate changes after edits
    Validate {
        base: String,
        #[arg(default_value = "HEAD")]
        head: String,
    },
    /// Explain a file's role in the graph
    Explain { file: String },
    /// Run a full indexing pass and emit a JSON summary
    #[command(name = "index-full", visible_alias = "index_full")]
    IndexFull {
        #[arg(long)]
        preset: Option<String>,
        /// Skip semantic vector refresh and rebuild only lexical search documents
        #[arg(long, default_value_t = false)]
        no_semantic: bool,
        /// Override the semantic backend used for index-time vector refresh
        #[arg(long)]
        semantic_backend: Option<String>,
        /// Force a full vector rebuild for the selected backend instead of reusing cached vectors
        #[arg(long, default_value_t = false)]
        reembed_all: bool,
    },
    /// Rebuild persisted call edges from indexed call sites
    #[command(name = "warm-calls", visible_alias = "warm_calls")]
    WarmCalls,
    /// Rebuild lexical search docs and optional semantic vectors from the cached graph
    #[command(name = "refresh-search", visible_alias = "refresh_search")]
    RefreshSearch {
        /// Skip semantic vector refresh and rebuild only lexical search documents
        #[arg(long, default_value_t = false)]
        no_semantic: bool,
        /// Override the semantic backend used for vector refresh
        #[arg(long)]
        semantic_backend: Option<String>,
        /// Force a full vector rebuild for the selected backend instead of reusing cached vectors
        #[arg(long, default_value_t = false)]
        reembed_all: bool,
    },
    /// Reviewing code changes - gives risk-scored analysis.
    #[command(name = "detect_changes", visible_alias = "detect-changes")]
    DetectChanges {
        base: String,
        #[arg(default_value = "HEAD")]
        head: String,
        #[arg(long, default_value = "json")]
        format: String,
    },
    /// Need source context for review - token-efficient.
    #[command(name = "get_review_context", visible_alias = "get-review-context")]
    GetReviewContext {
        task: String,
        #[arg(long, default_value = "4000")]
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
    /// Run benchmark and output phase timings as JSON
    #[command(name = "bench")]
    Bench {
        /// Run with --no-semantic (default: true)
        #[arg(long, default_value_t = true)]
        no_semantic: bool,
    },
    /// Run the MCP-compatible stdio server.
    Mcp {
        #[arg(long, default_value_t = 1_048_576)]
        max_request_bytes: usize,
    },
}

#[derive(Subcommand)]
enum InstallAction {
    /// Install RepoScry integration files
    Install {
        /// Overwrite files that already exist
        #[arg(long)]
        force: bool,
        /// Show what would be installed without writing files
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum RulesAction {
    Check,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let repo_root = PathBuf::from(&cli.repo_root).canonicalize()?;
    let reposcry_dir = repo_root.join(CACHE_DIR);
    let db_path = reposcry_dir.join(CACHE_DB);

    match cli.command {
        Commands::Init => cmd_init(&repo_root, &reposcry_dir, &db_path),
        Commands::Install {
            platform,
            force,
            dry_run,
        } => cmd_install(
            &repo_root,
            &reposcry_dir,
            &db_path,
            platform,
            force,
            dry_run,
        ),
        Commands::InstallMcp {
            platform,
            force,
            dry_run,
        } => cmd_install(
            &repo_root,
            &reposcry_dir,
            &db_path,
            platform,
            force,
            dry_run,
        ),
        Commands::Vscode { action } => cmd_install_action(
            &repo_root,
            &reposcry_dir,
            &db_path,
            InstallPlatform::Vscode,
            action,
        ),
        Commands::Cursor { action } => cmd_install_action(
            &repo_root,
            &reposcry_dir,
            &db_path,
            InstallPlatform::Cursor,
            action,
        ),
        Commands::Kiro { action } => cmd_install_action(
            &repo_root,
            &reposcry_dir,
            &db_path,
            InstallPlatform::Kiro,
            action,
        ),
        Commands::Antigravity { action } => cmd_install_action(
            &repo_root,
            &reposcry_dir,
            &db_path,
            InstallPlatform::Antigravity,
            action,
        ),
        Commands::Hooks { action } => cmd_install_action(
            &repo_root,
            &reposcry_dir,
            &db_path,
            InstallPlatform::Hooks,
            action,
        ),
        Commands::Index {
            preset,
            no_semantic,
            semantic_backend,
            reembed_all,
        } => cmd_index(
            &repo_root,
            &reposcry_dir,
            &db_path,
            preset.as_deref(),
            &SearchIndexOptions {
                semantic_backend,
                no_semantic,
                reembed_all,
            },
        ),
        Commands::Stats => cmd_stats(&db_path),
        Commands::Files { language } => cmd_files(&repo_root, &db_path, language),
        Commands::Symbols { file } => cmd_symbols(&db_path, &file),
        Commands::Deps { file } => cmd_deps(&repo_root, &db_path, &file),
        Commands::Rdeps { file } => cmd_rdeps(&repo_root, &db_path, &file),
        Commands::Diff { base, head } => cmd_diff(&repo_root, &base, &head),
        Commands::Context {
            task,
            budget,
            strict,
            full,
            format,
            max_symbols_per_file,
            show_omitted,
        } => cmd_context(
            &repo_root,
            &reposcry_dir,
            &db_path,
            &task,
            budget,
            strict,
            full,
            &format,
            max_symbols_per_file,
            show_omitted,
        ),
        Commands::Report { base, head, format } => {
            cmd_report(&repo_root, &db_path, &base, &head, &format)
        }
        Commands::Rules { action } => cmd_rules(&repo_root, &db_path, action),
        Commands::Validate { base, head } => cmd_validate(&repo_root, &db_path, &base, &head),
        Commands::Explain { file } => cmd_explain(&repo_root, &db_path, &file),
        Commands::IndexFull {
            preset,
            no_semantic,
            semantic_backend,
            reembed_all,
        } => cmd_index_full(
            &repo_root,
            &reposcry_dir,
            &db_path,
            preset.as_deref(),
            &SearchIndexOptions {
                semantic_backend,
                no_semantic,
                reembed_all,
            },
        ),
        Commands::WarmCalls => cmd_warm_calls(&repo_root, &db_path),
        Commands::RefreshSearch {
            no_semantic,
            semantic_backend,
            reembed_all,
        } => cmd_refresh_search(
            &repo_root,
            &db_path,
            &SearchIndexOptions {
                semantic_backend,
                no_semantic,
                reembed_all,
            },
        ),
        Commands::DetectChanges { base, head, format } => crg_cli::run_command(
            &repo_root,
            &db_path,
            crg_cli::Commands::DetectChanges { base, head, format },
        ),
        Commands::GetReviewContext {
            task,
            budget,
            strict,
            format,
        } => crg_cli::run_command(
            &repo_root,
            &db_path,
            crg_cli::Commands::GetReviewContext {
                task,
                budget,
                strict,
                format,
            },
        ),
        Commands::GetImpactRadius {
            target,
            depth,
            format,
        } => crg_cli::run_command(
            &repo_root,
            &db_path,
            crg_cli::Commands::GetImpactRadius {
                target,
                depth,
                format,
            },
        ),
        Commands::GetAffectedFlows { base, head, format } => crg_cli::run_command(
            &repo_root,
            &db_path,
            crg_cli::Commands::GetAffectedFlows { base, head, format },
        ),
        Commands::QueryGraph {
            query,
            no_runtime_calls,
            format,
        } => crg_cli::run_command(
            &repo_root,
            &db_path,
            crg_cli::Commands::QueryGraph {
                query,
                no_runtime_calls,
                format,
            },
        ),
        Commands::SemanticSearchNodes {
            query,
            kind,
            limit,
            semantic,
            semantic_backend,
            format,
        } => crg_cli::run_command(
            &repo_root,
            &db_path,
            crg_cli::Commands::SemanticSearchNodes {
                query,
                kind,
                limit,
                semantic,
                semantic_backend,
                format,
            },
        ),
        Commands::GetArchitectureOverview { format } => crg_cli::run_command(
            &repo_root,
            &db_path,
            crg_cli::Commands::GetArchitectureOverview { format },
        ),
        Commands::RefactorTool {
            action,
            target,
            replacement,
            format,
        } => crg_cli::run_command(
            &repo_root,
            &db_path,
            crg_cli::Commands::RefactorTool {
                action,
                target,
                replacement,
                format,
            },
        ),
        Commands::Bench { no_semantic } => cmd_bench(
            &repo_root,
            &reposcry_dir,
            &db_path,
            &SearchIndexOptions {
                semantic_backend: None,
                no_semantic,
                reembed_all: false,
            },
        ),
        Commands::Mcp { max_request_bytes } => crg_cli::run_command(
            &repo_root,
            &db_path,
            crg_cli::Commands::Mcp { max_request_bytes },
        ),
    }
}

#[derive(Debug, Serialize)]
struct FullIndexStepResult {
    name: String,
    ok: bool,
    detail: String,
}

#[derive(Debug, Serialize)]
struct FullIndexSummary {
    files: i64,
    symbols: i64,
    imports: i64,
    edges: i64,
    preset: Option<String>,
}

#[derive(Debug, Serialize)]
struct FullIndexOutput {
    tool: String,
    repo: String,
    steps: Vec<FullIndexStepResult>,
    summary: FullIndexSummary,
}

#[derive(Debug, Serialize)]
struct WarmCallsOutput {
    tool: String,
    repo: String,
    elapsed_ms: i64,
    persisted_call_sites: i64,
    persisted_symbol_call_edges: i64,
    persisted_file_call_edges: usize,
}

#[derive(Debug, Clone, Copy, Serialize)]
struct IndexTimings {
    scan_ms: u64,
    parse_ms: u64,
    edge_rebuild_ms: u64,
    search_rebuild_ms: u64,
    total_ms: u64,
}

struct SearchIndexOptions {
    semantic_backend: Option<String>,
    no_semantic: bool,
    reembed_all: bool,
}

fn ensure_reposcry_dir(reposcry_dir: &Path) -> anyhow::Result<()> {
    if !reposcry_dir.exists() {
        std::fs::create_dir_all(reposcry_dir)?;
    }
    Ok(())
}

fn cmd_init(repo_root: &Path, reposcry_dir: &Path, db_path: &Path) -> anyhow::Result<()> {
    info!("Initializing RepoScry in {}", repo_root.display());
    ensure_reposcry_dir(reposcry_dir)?;
    // Open database to initialize schema
    let _db = CacheDb::open(db_path)?;
    _db.set_config("repo_root", &repo_root.to_string_lossy())?;
    _db.set_config("version", env!("CARGO_PKG_VERSION"))?;
    _db.set_config("initialized_at", &chrono::Utc::now().to_rfc3339())?;
    // Create default ignore file if it doesn't exist
    let ignore_path = repo_root.join(IGNORE_FILE);
    if !ignore_path.exists() {
        let default_ignore = r#"# RepoScry ignore.

.node-version
.git/
node_modules/
target/
target-codex-test/
dist/
build/
.next/
.turbo/
coverage/
.cache/
graphify-out/
.reposcry/
public/static/charting_library/
*.min.js
*.map
*.lock
*.pdb
*.wasm
*.png
*.jpg
*.jpeg
*.webp
*.gif
*.svg
*.ico
*.mp4
*.mp3
*.wav
*.zip
*.tar
*.gz
*.pdf
package-lock.json
pnpm-lock.yaml
yarn.lock
Cargo.lock"#;
        std::fs::write(&ignore_path, default_ignore)?;
        info!("Created {}", IGNORE_FILE);
    }
    info!("Code review graph initialized.");
    Ok(())
}

fn cmd_install_action(
    repo_root: &Path,
    reposcry_dir: &Path,
    db_path: &Path,
    platform: InstallPlatform,
    action: InstallAction,
) -> anyhow::Result<()> {
    match action {
        InstallAction::Install { force, dry_run } => {
            cmd_install(repo_root, reposcry_dir, db_path, platform, force, dry_run)
        }
    }
}

fn cmd_install(
    repo_root: &Path,
    reposcry_dir: &Path,
    db_path: &Path,
    platform: InstallPlatform,
    force: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    if !reposcry_dir.exists() || !repo_root.join(IGNORE_FILE).exists() {
        cmd_init(repo_root, reposcry_dir, db_path)?;
    } else {
        ensure_reposcry_dir(reposcry_dir)?;
        let _db = CacheDb::open(db_path)?;
    }
    let summary = install_platform(repo_root, platform, InstallOptions { force, dry_run })?;
    println!("Installed RepoScry integration for {}", platform.label());
    if dry_run {
        println!("Dry run only. No files were written.");
    }
    for write in &summary.writes {
        println!("  {:16} {}", write.action, write.path.display());
    }
    println!();
    println!("Next commands:");
    println!("  reposcry index");
    println!("  reposcry context \"your task\"                # compact (4k budget)");
    println!("  reposcry context \"your task\" --budget 8000  # deep");
    println!("  reposcry context \"your task\" --budget 20000 # diagnostic/full");
    println!("  reposcry validate main...HEAD");
    Ok(())
}

fn cmd_index(
    repo_root: &Path,
    reposcry_dir: &Path,
    db_path: &Path,
    preset_name: Option<&str>,
    search_index_options: &SearchIndexOptions,
) -> anyhow::Result<()> {
    run_index(
        repo_root,
        reposcry_dir,
        db_path,
        preset_name,
        search_index_options,
    )?;
    Ok(())
}

fn run_index(
    repo_root: &Path,
    reposcry_dir: &Path,
    db_path: &Path,
    preset_name: Option<&str>,
    search_index_options: &SearchIndexOptions,
) -> anyhow::Result<IndexTimings> {
    info!("Indexing repository: {}", repo_root.display());
    let t_total = Instant::now();

    ensure_reposcry_dir(reposcry_dir)?;
    let db = CacheDb::open(db_path)?;
    let mut scanner = FileScanner::new(repo_root);
    let ignore_path = repo_root.join(IGNORE_FILE);
    if ignore_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&ignore_path) {
            for line in content.lines() {
                let line = line.trim();
                if !line.is_empty() && !line.starts_with('#') {
                    scanner = scanner.add_ignore_pattern(line);
                }
            }
        }
    }
    if let Some(name) = preset_name {
        if let Some(preset) = get_preset(name) {
            info!("Using preset: {}", name);
            scanner = scanner.with_preset(preset.clone());
        } else {
            warn!(
                "Unknown preset: {}. Available: nextjs, rust, tauri, monorepo, python",
                name
            );
        }
    }

    let t_scan = Instant::now();
    let files = scanner.scan()?;
    let scan_ms = t_scan.elapsed().as_millis() as u64;
    info!("Found {} files to index (scan: {}ms)", files.len(), scan_ms);

    let scanned_paths: HashSet<&str> = files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect();
    let cached_files = db.get_all_files()?;
    let mut deleted_count = 0usize;
    for cached_file in &cached_files {
        if !scanned_paths.contains(cached_file.path.as_str()) {
            db.delete_file(&cached_file.path)?;
            deleted_count += 1;
        }
    }
    if deleted_count > 0 {
        info!("Removed {} stale cached files", deleted_count);
    }

    let mut parsed_count = 0usize;
    let mut skipped_count = 0usize;
    let mut changed_file_ids: HashSet<i64> = HashSet::new();
    if deleted_count > 0 {
        changed_file_ids.insert(-1);
    }

    let t_parse = Instant::now();
    for file in &files {
        let source = match std::fs::read_to_string(&file.path) {
            Ok(s) => s,
            Err(e) => {
                warn!("Skipping unreadable file {}: {}", file.relative_path, e);
                continue;
            }
        };
        let hash = blake3::hash(source.as_bytes()).to_hex().to_string();
        let cached = db.get_file_by_path(&file.relative_path)?;
        let needs_parse = match cached.as_ref() {
            Some(existing) => existing.hash != hash,
            None => true,
        };
        if needs_parse {
            match parse_file(&file.relative_path, &source) {
                Ok(parsed) => {
                    let db_file_id = db.upsert_file(
                        &file.relative_path,
                        &file.language,
                        &hash,
                        file.size_bytes as i64,
                        parsed.loc as i64,
                    )?;
                    db.insert_symbols(db_file_id, &parsed.symbols)?;
                    db.insert_imports(db_file_id, &parsed.imports)?;
                    db.insert_call_sites(db_file_id, &parsed.calls)?;
                    changed_file_ids.insert(db_file_id);
                    parsed_count += 1;
                    trace!(
                        "Indexed: {} ({} symbols, {} imports, {} calls)",
                        file.relative_path,
                        parsed.symbols.len(),
                        parsed.imports.len(),
                        parsed.calls.len()
                    );
                }
                Err(e) => {
                    warn!("Failed to parse {}: {}", file.relative_path, e);
                    let db_file_id = db.upsert_file(
                        &file.relative_path,
                        &file.language,
                        &hash,
                        file.size_bytes as i64,
                        0,
                    )?;
                    db.insert_symbols(db_file_id, &[])?;
                    db.insert_imports(db_file_id, &[])?;
                    db.insert_call_sites(db_file_id, &[])?;
                    changed_file_ids.insert(db_file_id);
                }
            }
        } else {
            skipped_count += 1;
            trace!("Skipping unchanged: {}", file.relative_path);
        }
    }
    let parse_ms = t_parse.elapsed().as_millis() as u64;

    let graph_cache_current = db.get_config(GRAPH_CACHE_STATUS_KEY)?.as_deref() == Some("current");
    let search_index_current =
        db.get_config(SEARCH_INDEX_STATUS_KEY)?.as_deref() == Some("current");
    let has_new_or_changed_content = !changed_file_ids.is_empty();
    let rebuild_graph_cache = has_new_or_changed_content || !graph_cache_current;
    let rebuild_search_cache =
        has_new_or_changed_content || !search_index_current || search_index_options.reembed_all;

    let t_edge = Instant::now();
    if rebuild_graph_cache {
        db.set_config(GRAPH_CACHE_STATUS_KEY, "dirty")?;
        let changed = if has_new_or_changed_content {
            Some(&changed_file_ids)
        } else {
            None
        };
        rebuild_persisted_import_edges(&db, changed)?;
        rebuild_persisted_call_edges(&db, changed)?;
        db.set_config(GRAPH_CACHE_STATUS_KEY, "current")?;
    } else {
        info!("No file content changes detected; reusing persisted graph edges.");
    }
    let edge_rebuild_ms = t_edge.elapsed().as_millis() as u64;

    let t_search = Instant::now();
    if rebuild_search_cache {
        db.set_config(SEARCH_INDEX_STATUS_KEY, "dirty")?;
        rebuild_search_index(&db, repo_root, search_index_options)?;
        db.set_config(SEARCH_INDEX_STATUS_KEY, "current")?;
    } else {
        info!("No file content changes detected; reusing search index.");
    }
    let search_rebuild_ms = t_search.elapsed().as_millis() as u64;

    let total_ms = t_total.elapsed().as_millis() as u64;

    let preset_name = preset_name.unwrap_or("none");
    db.set_config("last_indexed_at", &chrono::Utc::now().to_rfc3339())?;
    db.set_config("preset", preset_name)?;
    db.set_config("files_count", &files.len().to_string())?;
    info!(
        "Indexing complete. {} files indexed, {} parsed, {} unchanged, {} deleted, {} imports, {} calls, {} symbol call edges, {} file edges.",
        db.file_count()?,
        parsed_count,
        skipped_count,
        deleted_count,
        db.import_count()?,
        db.call_site_count()?,
        db.symbol_edge_count()?,
        db.edge_count()?
    );
    info!(
        "Phase timings: scan={}ms parse={}ms edge_rebuild={}ms search_rebuild={}ms total={}ms",
        scan_ms, parse_ms, edge_rebuild_ms, search_rebuild_ms, total_ms
    );
    Ok(IndexTimings {
        scan_ms,
        parse_ms,
        edge_rebuild_ms,
        search_rebuild_ms,
        total_ms,
    })
}

fn cmd_index_full(
    repo_root: &Path,
    reposcry_dir: &Path,
    db_path: &Path,
    preset_name: Option<&str>,
    search_index_options: &SearchIndexOptions,
) -> anyhow::Result<()> {
    cmd_index(
        repo_root,
        reposcry_dir,
        db_path,
        preset_name,
        search_index_options,
    )?;
    let db = CacheDb::open(db_path)?;
    let output = FullIndexOutput {
        tool: "reposcry".to_string(),
        repo: repo_root.display().to_string(),
        steps: vec![FullIndexStepResult {
            name: "index".to_string(),
            ok: true,
            detail: "reposcry index completed successfully".to_string(),
        }],
        summary: FullIndexSummary {
            files: db.file_count()?,
            symbols: db.symbol_count()?,
            imports: db.import_count()?,
            edges: db.edge_count()?,
            preset: preset_name
                .map(str::to_string)
                .or_else(|| db.get_config("preset").ok().flatten()),
        },
    };

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn cmd_warm_calls(repo_root: &Path, db_path: &Path) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let db = CacheDb::open(db_path)?;
    rebuild_persisted_call_edges(&db, None)?;
    let output = WarmCallsOutput {
        tool: "reposcry".to_string(),
        repo: repo_root.display().to_string(),
        elapsed_ms: started.elapsed().as_millis() as i64,
        persisted_call_sites: db.call_site_count()?,
        persisted_symbol_call_edges: db.symbol_edge_count()?,
        persisted_file_call_edges: db.get_edges_by_kind(EdgeKind::Calls)?.len(),
    };
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn cmd_refresh_search(
    repo_root: &Path,
    db_path: &Path,
    options: &SearchIndexOptions,
) -> anyhow::Result<()> {
    let db = CacheDb::open(db_path)?;
    db.set_config(SEARCH_INDEX_STATUS_KEY, "dirty")?;
    rebuild_search_index(&db, repo_root, options)?;
    db.set_config(SEARCH_INDEX_STATUS_KEY, "current")?;
    Ok(())
}

fn rebuild_search_index(
    db: &CacheDb,
    repo_root: &Path,
    options: &SearchIndexOptions,
) -> anyhow::Result<()> {
    let files = db.get_all_files()?;
    db.clear_search_index()?;
    if options.no_semantic {
        db.clear_search_vectors(None)?;
    }
    let semantic_backend = (!options.no_semantic)
        .then(|| configured_semantic_backend(db, options.semantic_backend.as_deref()))
        .transpose()?;
    if let Some(semantic_backend) = semantic_backend.as_ref() {
        if options.reembed_all {
            db.clear_search_vectors(Some(semantic_backend.name()))?;
        }
    }
    let mut inserted_vectors = 0usize;
    let mut reused_vectors = 0usize;
    for file in &files {
        let imports = db.get_imports_by_file(file.id)?;
        let imports_text = imports
            .iter()
            .flat_map(|import| {
                let mut parts = vec![import.target.clone()];
                parts.extend(import.imported_names.clone());
                parts
            })
            .collect::<Vec<_>>()
            .join(" ");
        let source = std::fs::read_to_string(repo_root.join(&file.path)).unwrap_or_default();
        db.insert_search_document(
            1_000_000_000_i64 + file.id,
            &file.path,
            "file",
            &file.path,
            None,
            None,
            &imports_text,
            &source,
        )?;
        if let Some(semantic_backend) = semantic_backend.as_ref() {
            let node_id = 1_000_000_000_i64 + file.id;
            if options.reembed_all || !db.has_search_vector(node_id, semantic_backend.name())? {
                db.insert_search_vector(
                    node_id,
                    &file.path,
                    "file",
                    &file.path,
                    None,
                    semantic_backend.name(),
                    &semantic_backend
                        .embed_text(&format!("{} {} {}", file.path, imports_text, source))?,
                )?;
                inserted_vectors += 1;
            } else {
                reused_vectors += 1;
            }
        }
        for symbol in db.get_symbols_by_file(file.id)? {
            let Some(symbol_id) = symbol.id else {
                continue;
            };
            db.insert_search_document(
                symbol_id,
                &file.path,
                &symbol.kind,
                &symbol.name,
                symbol.signature.as_deref(),
                symbol.doc_comment.as_deref(),
                &imports_text,
                "",
            )?;
            if let Some(semantic_backend) = semantic_backend.as_ref() {
                if options.reembed_all
                    || !db.has_search_vector(symbol_id, semantic_backend.name())?
                {
                    db.insert_search_vector(
                        symbol_id,
                        &file.path,
                        &symbol.kind,
                        &symbol.name,
                        symbol.signature.as_deref(),
                        semantic_backend.name(),
                        &semantic_backend.embed_text(&format!(
                            "{} {} {} {}",
                            symbol.name,
                            symbol.signature.as_deref().unwrap_or(""),
                            symbol.doc_comment.as_deref().unwrap_or(""),
                            imports_text
                        ))?,
                    )?;
                    inserted_vectors += 1;
                } else {
                    reused_vectors += 1;
                }
            }
        }
    }
    if let Some(semantic_backend) = semantic_backend.as_ref() {
        db.prune_search_vectors_to_index(semantic_backend.name())?;
        db.set_config("semantic_backend", semantic_backend.name())?;
        info!(
            "Search index refreshed with backend {} ({} vectors inserted, {} reused).",
            semantic_backend.name(),
            inserted_vectors,
            reused_vectors
        );
    } else {
        info!("Search index refreshed without semantic vectors.");
    }
    Ok(())
}

enum SemanticBackend {
    LocalHash,
    Ollama { url: String, model: String },
    Fastembed { model: Mutex<TextEmbedding> },
    Candle { model: CandleModel },
}

enum CandleModel {
    NomicV2Moe(NomicV2MoeTextEmbedding),
    Qwen3(Qwen3TextEmbedding),
}

impl SemanticBackend {
    fn name(&self) -> &str {
        match self {
            Self::LocalHash => LOCAL_SEMANTIC_BACKEND,
            Self::Ollama { .. } => OLLAMA_BACKEND,
            Self::Fastembed { .. } => FASTEMBED_BACKEND,
            Self::Candle { .. } => CANDLE_BACKEND,
        }
    }

    fn embed_text(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        match self {
            Self::LocalHash => Ok(local_text_embedding(text)),
            Self::Ollama { url, model } => ollama_embedding(url, model, text),
            Self::Fastembed { model } => fastembed_embedding(model, text),
            Self::Candle { model } => candle_embedding(model, text),
        }
    }
}

fn configured_semantic_backend(
    db: &CacheDb,
    override_backend: Option<&str>,
) -> anyhow::Result<SemanticBackend> {
    let backend_name = override_backend
        .map(str::to_string)
        .or_else(|| env::var("REPOSCRY_SEMANTIC_BACKEND").ok())
        .or_else(|| db.get_config("semantic_backend").ok().flatten())
        .unwrap_or_else(|| LOCAL_SEMANTIC_BACKEND.to_string());
    match backend_name.as_str() {
        OLLAMA_BACKEND => Ok(SemanticBackend::Ollama {
            url: env::var("REPOSCRY_OLLAMA_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:11434/api/embeddings".to_string()),
            model: env::var("REPOSCRY_OLLAMA_MODEL")
                .unwrap_or_else(|_| "nomic-embed-text".to_string()),
        }),
        FASTEMBED_BACKEND => {
            let model_name = env::var("REPOSCRY_FASTEMBED_MODEL")
                .unwrap_or_else(|_| "AllMiniLML6V2".to_string());
            Ok(SemanticBackend::Fastembed {
                model: Mutex::new(build_fastembed_model(&model_name)?),
            })
        }
        CANDLE_BACKEND => Ok(SemanticBackend::Candle {
            model: build_candle_model(
                &env::var("REPOSCRY_CANDLE_MODEL").unwrap_or_else(|_| "qwen3".to_string()),
                &env::var("REPOSCRY_CANDLE_REPO")
                    .unwrap_or_else(|_| "Qwen/Qwen3-Embedding-0.6B".to_string()),
                env::var("REPOSCRY_CANDLE_MAX_LENGTH")
                    .ok()
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(512),
            )?,
        }),
        _ => Ok(SemanticBackend::LocalHash),
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

fn ollama_embedding(url: &str, model: &str, text: &str) -> anyhow::Result<Vec<f32>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let response = client
        .post(url)
        .json(&serde_json::json!({
            "model": model,
            "prompt": text,
        }))
        .send()?
        .error_for_status()?;
    let payload: serde_json::Value = response.json()?;
    let embedding = payload
        .get("embedding")
        .and_then(|value| value.as_array())
        .ok_or_else(|| anyhow::anyhow!("ollama response missing embedding array"))?;
    let vector = embedding
        .iter()
        .map(|value| value.as_f64().unwrap_or_default() as f32)
        .collect::<Vec<_>>();
    if vector.is_empty() {
        return Err(anyhow::anyhow!("ollama returned an empty embedding"));
    }
    Ok(vector)
}

fn build_fastembed_model(model_name: &str) -> anyhow::Result<TextEmbedding> {
    let model = match model_name {
        "BGESmallENV15" => EmbeddingModel::BGESmallENV15,
        "BGEBaseENV15" => EmbeddingModel::BGEBaseENV15,
        "AllMiniLML6V2" => EmbeddingModel::AllMiniLML6V2,
        "AllMiniLML12V2" => EmbeddingModel::AllMiniLML12V2,
        other => {
            return Err(anyhow::anyhow!(
                "unsupported REPOSCRY_FASTEMBED_MODEL `{}`; supported values: AllMiniLML6V2, AllMiniLML12V2, BGEBaseENV15, BGESmallENV15",
                other
            ))
        }
    };
    let mut options = InitOptions::new(model);
    let cache_dir: PathBuf = env::var("REPOSCRY_FASTEMBED_CACHE_DIR")
        .or_else(|_| env::var("HF_HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".reposcry")
                .join("hf-home")
        });
    if env::var("HF_HOME").is_err() {
        env::set_var("HF_HOME", &cache_dir);
    }
    std::fs::create_dir_all(&cache_dir)?;
    options = options.with_cache_dir(cache_dir);
    Ok(TextEmbedding::try_new(options)?)
}

fn fastembed_embedding(model: &Mutex<TextEmbedding>, text: &str) -> anyhow::Result<Vec<f32>> {
    let mut model = model
        .lock()
        .map_err(|_| anyhow::anyhow!("fastembed model mutex poisoned"))?;
    let embeddings = model.embed(vec![text], None)?;
    embeddings
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("fastembed returned no embedding for input"))
}

fn build_candle_model(
    model_kind: &str,
    repo_id: &str,
    max_length: usize,
) -> anyhow::Result<CandleModel> {
    let _cache_dir = ensure_hf_home("REPOSCRY_CANDLE_CACHE_DIR")?;
    match model_kind {
        "nomic-v2-moe" | "nomic_v2_moe" | "nomic" => {
            NomicV2MoeTextEmbedding::from_hf(repo_id, &Device::Cpu, DType::F32, max_length)
                .map(CandleModel::NomicV2Moe)
                .map_err(|error| anyhow::anyhow!(error.to_string()))
        }
        "qwen3" | "qwen" => {
            Qwen3TextEmbedding::from_hf(repo_id, &Device::Cpu, DType::F32, max_length)
                .map(CandleModel::Qwen3)
                .map_err(|error| anyhow::anyhow!(error.to_string()))
        }
        other => Err(anyhow::anyhow!(
            "unsupported REPOSCRY_CANDLE_MODEL `{}`; supported values: qwen3, nomic-v2-moe",
            other
        )),
    }
}

fn candle_embedding(model: &CandleModel, text: &str) -> anyhow::Result<Vec<f32>> {
    let embeddings = match model {
        CandleModel::NomicV2Moe(model) => model
            .embed(&[text])
            .map_err(|error| anyhow::anyhow!(error.to_string()))?,
        CandleModel::Qwen3(model) => model
            .embed(&[text])
            .map_err(|error| anyhow::anyhow!(error.to_string()))?,
    };
    embeddings
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("candle backend returned no embedding for input"))
}

fn ensure_hf_home(cache_env_var: &str) -> anyhow::Result<PathBuf> {
    let cache_dir: PathBuf = env::var(cache_env_var)
        .or_else(|_| env::var("HF_HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".reposcry")
                .join("hf-home")
        });
    if env::var("HF_HOME").is_err() {
        env::set_var("HF_HOME", &cache_dir);
    }
    std::fs::create_dir_all(&cache_dir)?;
    Ok(cache_dir)
}

fn rebuild_persisted_import_edges(
    db: &CacheDb,
    changed_file_ids: Option<&HashSet<i64>>,
) -> anyhow::Result<()> {
    let files = db.get_all_files()?;
    let file_paths: HashSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let file_by_path: HashMap<&str, &CachedFile> =
        files.iter().map(|f| (f.path.as_str(), f)).collect();

    if let Some(changed_ids) = changed_file_ids {
        if !changed_ids.is_empty() {
            for file in &files {
                if changed_ids.contains(&file.id) {
                    db.delete_edges_by_source(file.id, EdgeKind::Imports)?;
                }
            }
        }
    } else {
        db.clear_edges_by_kind(EdgeKind::Imports)?;
    }

    let sources: Vec<&CachedFile> = match changed_file_ids {
        Some(changed_ids) if !changed_ids.is_empty() => files
            .iter()
            .filter(|f| changed_ids.contains(&f.id))
            .collect(),
        _ => files.iter().collect(),
    };

    for file in &sources {
        let imports = db.get_imports_by_file(file.id)?;
        for import in imports {
            if let Some(target_path) =
                resolve_import_target_with_paths(&file.path, &import.target, &file_paths)
            {
                if let Some(&target_file) = file_by_path.get(target_path.as_str()) {
                    db.insert_edge(
                        file.id,
                        Some(target_file.id),
                        Some(&target_path),
                        EdgeKind::Imports,
                        1.0,
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn rebuild_persisted_call_edges(
    db: &CacheDb,
    changed_file_ids: Option<&HashSet<i64>>,
) -> anyhow::Result<()> {
    let files = db.get_all_files()?;
    let files_by_id: HashMap<i64, &CachedFile> = files.iter().map(|file| (file.id, file)).collect();
    let file_by_path: HashMap<&str, &CachedFile> =
        files.iter().map(|f| (f.path.as_str(), f)).collect();
    let file_paths: HashSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let mut symbols_by_file = HashMap::new();
    let mut imports_by_file = HashMap::new();
    let mut global_symbol_candidates: HashMap<String, Vec<reposcry_graph::symbol::Symbol>> =
        HashMap::new();
    for file in &files {
        let symbols = db.get_symbols_by_file(file.id)?;
        let imports = db.get_imports_by_file(file.id)?;
        for symbol in &symbols {
            push_global_symbol_candidate(&mut global_symbol_candidates, &symbol.name, symbol);
            if let Some(short) = symbol.name.rsplit("::").next() {
                push_global_symbol_candidate(&mut global_symbol_candidates, short, symbol);
            }
        }
        symbols_by_file.insert(file.id, symbols);
        imports_by_file.insert(file.id, imports);
    }

    if let Some(changed_ids) = changed_file_ids {
        if !changed_ids.is_empty() {
            for file in &files {
                if changed_ids.contains(&file.id) {
                    db.delete_edges_by_source(file.id, EdgeKind::Calls)?;
                    db.delete_symbol_edges_by_source(file.id, EdgeKind::Calls.as_str())?;
                }
            }
        }
    } else {
        db.clear_edges_by_kind(EdgeKind::Calls)?;
        db.clear_symbol_edges_by_kind(EdgeKind::Calls.as_str())?;
    }

    let sources: Vec<&CachedFile> = match changed_file_ids {
        Some(changed_ids) if !changed_ids.is_empty() => files
            .iter()
            .filter(|f| changed_ids.contains(&f.id))
            .collect(),
        _ => files.iter().collect(),
    };

    let mut symbol_edges = Vec::<CachedSymbolEdge>::new();
    let mut file_edges = HashSet::<(i64, i64)>::new();

    for file in &sources {
        let call_sites = db.get_call_sites_by_file(file.id)?;
        let Some(file_symbols) = symbols_by_file.get(&file.id) else {
            continue;
        };
        let file_imports = imports_by_file
            .get(&file.id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let Some(source_file) = files_by_id.get(&file.id) else {
            continue;
        };
        for call_site in &call_sites {
            let Some(source_symbol) = resolve_caller_symbol(file_symbols, call_site) else {
                continue;
            };
            let Some((target_symbol, strategy)) = resolve_callee_symbol_with_paths(
                file_symbols,
                &global_symbol_candidates,
                file_imports,
                &source_file.path,
                &file_paths,
                call_site,
            ) else {
                continue;
            };
            let Some(source_symbol_id) = source_symbol.id else {
                continue;
            };
            let Some(target_symbol_id) = target_symbol.id else {
                continue;
            };
            let target_file_id = file_by_path
                .get(target_symbol.file_path.as_str())
                .map(|f| f.id)
                .unwrap_or(file.id);
            symbol_edges.push(CachedSymbolEdge {
                id: 0,
                source_symbol_id,
                target_symbol_id,
                source_file_id: file.id,
                target_file_id,
                kind: EdgeKind::Calls.as_str().to_string(),
                line: call_site.line,
                confidence: call_site.confidence,
                resolution_strategy: Some(strategy),
            });
            if let Some(&target_file) = file_by_path.get(target_symbol.file_path.as_str()) {
                file_edges.insert((file.id, target_file.id));
            }
        }
    }

    db.insert_symbol_edges(&symbol_edges)?;
    for (source_file_id, target_file_id) in file_edges {
        let target_path = files_by_id
            .get(&target_file_id)
            .map(|file| file.path.as_str());
        db.insert_edge(
            source_file_id,
            Some(target_file_id),
            target_path,
            EdgeKind::Calls,
            1.0,
        )?;
    }
    Ok(())
}

fn push_global_symbol_candidate(
    global_symbol_candidates: &mut HashMap<String, Vec<reposcry_graph::symbol::Symbol>>,
    key: &str,
    symbol: &reposcry_graph::symbol::Symbol,
) {
    let entry = global_symbol_candidates.entry(key.to_string()).or_default();
    if entry.iter().any(|candidate| {
        candidate.id == symbol.id
            && candidate.file_path == symbol.file_path
            && candidate.name == symbol.name
    }) {
        return;
    }
    entry.push(symbol.clone());
}

fn resolve_caller_symbol<'a>(
    file_symbols: &'a [reposcry_graph::symbol::Symbol],
    call_site: &CachedCallSite,
) -> Option<&'a reposcry_graph::symbol::Symbol> {
    file_symbols
        .iter()
        .filter(|symbol| symbol.name == call_site.caller)
        .find(|symbol| symbol.start_line <= call_site.line && symbol.end_line >= call_site.line)
        .or_else(|| {
            file_symbols
                .iter()
                .filter(|symbol| {
                    symbol.start_line <= call_site.line && symbol.end_line >= call_site.line
                })
                .min_by_key(|symbol| symbol.end_line.saturating_sub(symbol.start_line))
        })
}

fn resolve_callee_symbol(
    file_symbols: &[reposcry_graph::symbol::Symbol],
    global_symbol_candidates: &HashMap<String, Vec<reposcry_graph::symbol::Symbol>>,
    file_imports: &[CachedImport],
    source_path: &str,
    files: &[CachedFile],
    call_site: &CachedCallSite,
) -> Option<(reposcry_graph::symbol::Symbol, String)> {
    let file_paths: HashSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
    resolve_callee_symbol_with_paths(
        file_symbols,
        global_symbol_candidates,
        file_imports,
        source_path,
        &file_paths,
        call_site,
    )
}

fn resolve_callee_symbol_with_paths<'a>(
    file_symbols: &'a [reposcry_graph::symbol::Symbol],
    global_symbol_candidates: &'a HashMap<String, Vec<reposcry_graph::symbol::Symbol>>,
    file_imports: &[CachedImport],
    source_path: &str,
    file_paths: &HashSet<&str>,
    call_site: &CachedCallSite,
) -> Option<(reposcry_graph::symbol::Symbol, String)> {
    if let Some(symbol) = file_symbols.iter().find(|symbol| {
        symbol.name == call_site.callee
            || symbol.name.rsplit("::").next() == Some(call_site.callee.as_str())
    }) {
        return Some((symbol.clone(), "same_file".to_string()));
    }
    let candidates = dedup_symbol_candidates(global_symbol_candidates.get(&call_site.callee)?);
    if candidates.len() == 1 {
        return Some((candidates[0].clone(), "unique_global".to_string()));
    }
    let resolved_import_targets = resolved_import_target_paths_with_paths(
        file_imports,
        source_path,
        file_paths,
        &call_site.callee,
    );
    let import_candidates: Vec<_> = candidates
        .into_iter()
        .filter(|candidate| resolved_import_targets.contains(&candidate.file_path))
        .collect();
    if import_candidates.len() == 1 {
        return Some((import_candidates[0].clone(), "import_resolved".to_string()));
    }
    None
}

fn dedup_symbol_candidates(
    candidates: &[reposcry_graph::symbol::Symbol],
) -> Vec<reposcry_graph::symbol::Symbol> {
    let mut deduped = Vec::new();
    for candidate in candidates {
        if deduped
            .iter()
            .any(|existing: &reposcry_graph::symbol::Symbol| {
                existing.id == candidate.id
                    && existing.file_path == candidate.file_path
                    && existing.name == candidate.name
            })
        {
            continue;
        }
        deduped.push(candidate.clone());
    }
    deduped
}

fn resolved_import_target_paths(
    file_imports: &[CachedImport],
    source_path: &str,
    files: &[CachedFile],
    callee: &str,
) -> HashSet<String> {
    let file_paths: HashSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
    resolved_import_target_paths_with_paths(file_imports, source_path, &file_paths, callee)
}

fn resolved_import_target_paths_with_paths(
    file_imports: &[CachedImport],
    source_path: &str,
    file_paths: &HashSet<&str>,
    callee: &str,
) -> HashSet<String> {
    file_imports
        .iter()
        .filter(|import| {
            import.imported_names.is_empty()
                || import
                    .imported_names
                    .iter()
                    .filter_map(|name| name.strip_prefix("* as ").or(Some(name.as_str())))
                    .any(|name| name == callee)
        })
        .filter_map(|import| {
            resolve_import_target_with_paths(source_path, &import.target, file_paths)
        })
        .collect()
}

fn resolve_import_target(
    source_path: &str,
    raw_target: &str,
    files: &[CachedFile],
) -> Option<String> {
    let file_paths: HashSet<&str> = files.iter().map(|file| file.path.as_str()).collect();
    resolve_import_target_with_paths(source_path, raw_target, &file_paths)
}

fn resolve_import_target_with_paths(
    source_path: &str,
    raw_target: &str,
    file_paths: &HashSet<&str>,
) -> Option<String> {
    let raw_target = raw_target
        .trim()
        .trim_matches(';')
        .trim_matches('"')
        .trim_matches('\'');
    if raw_target.is_empty() {
        return None;
    }

    if raw_target.starts_with('.') {
        let parent = Path::new(source_path)
            .parent()
            .unwrap_or_else(|| Path::new(""));
        let base = normalize_relative_path(parent.join(raw_target));
        return find_candidate_path(&base, file_paths);
    }
    if let Some(rest) = raw_target.strip_prefix("@/") {
        let candidates = [
            find_candidate_path(rest, file_paths),
            find_candidate_path(&format!("src/{}", rest), file_paths),
        ];
        return candidates.into_iter().flatten().next();
    }
    if let Some(rest) = raw_target.strip_prefix("~/") {
        return find_candidate_path(rest, file_paths);
    }
    if let Some(resolved) = resolve_workspace_package_import_target(raw_target, file_paths) {
        return Some(resolved);
    }
    resolve_rust_import_target(source_path, raw_target, file_paths)
}

fn resolve_workspace_package_import_target(
    raw_target: &str,
    file_paths: &HashSet<&str>,
) -> Option<String> {
    let segments: Vec<&str> = raw_target
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.is_empty() {
        return None;
    }

    let (package_name, subpath_segments): (&str, &[&str]) = if raw_target.starts_with('@') {
        if segments.len() < 2 {
            return None;
        }
        (segments[1], &segments[2..])
    } else {
        (segments[0], &segments[1..])
    };

    let candidate_roots = [
        format!("packages/{}", package_name),
        format!("apps/{}", package_name),
        format!("crates/{}", package_name),
        package_name.to_string(),
    ];

    for root in candidate_roots {
        if subpath_segments.is_empty() {
            if let Some(resolved) = find_candidate_path(&format!("{}/src/index", root), file_paths)
            {
                return Some(resolved);
            }
            if let Some(resolved) = find_candidate_path(&format!("{}/index", root), file_paths) {
                return Some(resolved);
            }
            continue;
        }

        let subpath = subpath_segments.join("/");
        if let Some(resolved) =
            find_candidate_path(&format!("{}/src/{}", root, subpath), file_paths)
        {
            return Some(resolved);
        }
        if let Some(resolved) = find_candidate_path(&format!("{}/{}", root, subpath), file_paths) {
            return Some(resolved);
        }
    }

    None
}

fn resolve_rust_import_target(
    source_path: &str,
    raw_target: &str,
    file_paths: &HashSet<&str>,
) -> Option<String> {
    let cleaned = raw_target
        .split('{')
        .next()
        .unwrap_or(raw_target)
        .trim_end_matches(':')
        .trim_end_matches(':')
        .trim();
    if cleaned.is_empty() || !cleaned.contains("::") {
        return None;
    }
    let source = Path::new(source_path);
    let source_parent = source.parent().unwrap_or_else(|| Path::new(""));

    if let Some(rest) = cleaned.strip_prefix("crate::") {
        if let Some(src_root) = rust_src_root_for(source_path) {
            let segments: Vec<&str> = rest
                .split("::")
                .filter(|segment| !segment.is_empty())
                .collect();
            return find_rust_module_candidate(&src_root, &segments, file_paths);
        }
    }
    if let Some(rest) = cleaned.strip_prefix("self::") {
        let root = normalize_relative_path(source_parent.to_path_buf());
        let segments: Vec<&str> = rest
            .split("::")
            .filter(|segment| !segment.is_empty())
            .collect();
        return find_rust_module_candidate(&root, &segments, file_paths);
    }
    if let Some(rest) = cleaned.strip_prefix("super::") {
        let parent = source_parent.parent().unwrap_or_else(|| Path::new(""));
        let root = normalize_relative_path(parent.to_path_buf());
        let segments: Vec<&str> = rest
            .split("::")
            .filter(|segment| !segment.is_empty())
            .collect();
        return find_rust_module_candidate(&root, &segments, file_paths);
    }

    let mut parts = cleaned.split("::");
    let first = parts.next()?;
    let remainder: Vec<&str> = parts.collect();
    let crate_dir = first.replace('_', "-");
    let workspace_src = format!("crates/{}/src", crate_dir);
    if remainder.is_empty() {
        return find_candidate_path(&workspace_src, file_paths);
    }
    find_rust_module_candidate(&workspace_src, &remainder, file_paths)
        .or_else(|| find_candidate_path(&workspace_src, file_paths))
}

fn find_rust_module_candidate(
    root: &str,
    segments: &[&str],
    file_paths: &HashSet<&str>,
) -> Option<String> {
    for len in (1..=segments.len()).rev() {
        let base = format!(
            "{}/{}",
            root.trim_end_matches('/'),
            segments[..len].join("/")
        );
        if let Some(path) = find_candidate_path(&base, file_paths) {
            return Some(path);
        }
    }
    find_candidate_path(root, file_paths)
}

fn rust_src_root_for(source_path: &str) -> Option<String> {
    if let Some((prefix, _)) = source_path.split_once("/src/") {
        Some(format!("{}/src", prefix))
    } else if source_path.starts_with("src/") {
        Some("src".to_string())
    } else {
        None
    }
}

fn find_candidate_path(base: &str, file_paths: &HashSet<&str>) -> Option<String> {
    let normalized = normalize_slashes(base.trim_matches('/'));
    let candidates = candidate_paths(&normalized);
    candidates
        .into_iter()
        .find(|candidate| file_paths.contains(candidate.as_str()))
}

fn candidate_paths(base: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let base = base.trim_end_matches('/');
    if !base.is_empty() {
        candidates.push(base.to_string());
    }
    let extensions = [
        "ts", "tsx", "js", "jsx", "rs", "py", "json", "toml", "yaml", "yml",
    ];
    let has_known_extension = Path::new(base).extension().is_some();
    if !has_known_extension {
        for ext in extensions {
            candidates.push(format!("{}.{}", base, ext));
        }
        for ext in extensions {
            candidates.push(format!("{}/index.{}", base, ext));
        }
        candidates.push(format!("{}/mod.rs", base));
    }
    candidates
}

fn normalize_relative_path(path: PathBuf) -> String {
    let mut parts: Vec<String> = Vec::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                parts.pop();
            }
            Component::CurDir => {}
            Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
            _ => {}
        }
    }
    parts.join("/")
}

fn normalize_slashes(path: &str) -> String {
    path.replace('\\', "/")
}

#[derive(Debug, Serialize)]
struct BenchResult {
    repo: String,
    commit: String,
    files_seen: usize,
    files_indexed: i64,
    unchanged_files: usize,
    changed_files: usize,
    parsed_count: usize,
    deleted_count: usize,
    scan_ms: u64,
    parse_ms: u64,
    db_write_ms: u64,
    edge_rebuild_ms: u64,
    search_rebuild_ms: u64,
    total_ms: u64,
    db_bytes: u64,
    symbols: i64,
    imports: i64,
    call_sites: i64,
    symbol_call_edges: i64,
    file_edges: i64,
}

fn cmd_bench(
    repo_root: &Path,
    reposcry_dir: &Path,
    db_path: &Path,
    search_index_options: &SearchIndexOptions,
) -> anyhow::Result<()> {
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    // Cold start: delete any existing cache
    if reposcry_dir.exists() {
        std::fs::remove_dir_all(reposcry_dir)?;
    }

    let timings = run_index(repo_root, reposcry_dir, db_path, None, search_index_options)?;

    let db = CacheDb::open(db_path)?;
    let result = BenchResult {
        repo: repo_root.display().to_string(),
        commit,
        files_seen: db.file_count()? as usize,
        files_indexed: db.file_count()?,
        unchanged_files: 0,
        changed_files: 0,
        parsed_count: 0,
        deleted_count: 0,
        scan_ms: timings.scan_ms,
        parse_ms: timings.parse_ms,
        db_write_ms: 0,
        edge_rebuild_ms: timings.edge_rebuild_ms,
        search_rebuild_ms: timings.search_rebuild_ms,
        total_ms: timings.total_ms,
        db_bytes: std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0),
        symbols: db.symbol_count()?,
        imports: db.import_count()?,
        call_sites: db.call_site_count()?,
        symbol_call_edges: db.symbol_edge_count()?,
        file_edges: db.edge_count()?,
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn cmd_stats(db_path: &Path) -> anyhow::Result<()> {
    let db = CacheDb::open(db_path)?;
    let count = db.file_count()?;
    let stats = db.language_stats()?;
    println!("Files indexed: {}", count);
    println!("Symbols indexed: {}", db.symbol_count()?);
    println!("Imports indexed: {}", db.import_count()?);
    println!("Resolved import edges: {}", db.edge_count()?);
    println!();
    if stats.is_empty() {
        println!("No language data. Run `reposcry index` first.");
    } else {
        println!("Languages:");
        for (lang, cnt) in &stats {
            println!("  {}: {}", lang, cnt);
        }
    }
    Ok(())
}

fn cmd_files(_repo_root: &Path, db_path: &Path, language: Option<String>) -> anyhow::Result<()> {
    let db = CacheDb::open(db_path)?;
    let files = db.get_all_files()?;
    for file in &files {
        if let Some(ref lang) = language {
            if file.language != *lang {
                continue;
            }
        }
        println!("{}", file.path);
    }
    if files.is_empty() {
        println!("No files indexed. Run `reposcry index` first.");
    }
    Ok(())
}

fn cmd_symbols(db_path: &Path, file: &str) -> anyhow::Result<()> {
    let db = CacheDb::open(db_path)?;
    match db.get_file_by_path(file)? {
        Some(cached) => {
            let symbols = db.get_symbols_by_file(cached.id)?;
            if symbols.is_empty() {
                println!("No symbols found in {}", file);
            } else {
                for sym in &symbols {
                    let vis = sym.visibility.as_deref().unwrap_or("");
                    let sig = sym.signature.as_deref().unwrap_or("");
                    println!(
                        "  {} {} {} (line {})",
                        vis, sym.kind, sym.name, sym.start_line
                    );
                    if !sig.is_empty() {
                        println!("    signature: {}", sig);
                    }
                }
            }
        }
        None => {
            println!("File not indexed: {}", file);
        }
    }
    Ok(())
}

fn cmd_deps(_repo_root: &Path, db_path: &Path, file: &str) -> anyhow::Result<()> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let mut deps: Vec<String> = graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Imports)
        .filter(|e| {
            graph
                .nodes
                .get(&e.source_id)
                .and_then(|n| n.file_path.as_deref())
                == Some(file)
        })
        .filter_map(|e| {
            graph
                .nodes
                .get(&e.target_id)
                .and_then(|n| n.file_path.clone())
        })
        .collect();
    deps.sort();
    deps.dedup();
    if deps.is_empty() {
        println!("{} has no indexed dependencies.", file);
    } else {
        println!("{} depends on:", file);
        for dep in deps {
            println!("  - {}", dep);
        }
    }
    Ok(())
}

fn cmd_rdeps(_repo_root: &Path, db_path: &Path, file: &str) -> anyhow::Result<()> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let mut rdeps: Vec<String> = graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Imports)
        .filter(|e| {
            graph
                .nodes
                .get(&e.target_id)
                .and_then(|n| n.file_path.as_deref())
                == Some(file)
        })
        .filter_map(|e| {
            graph
                .nodes
                .get(&e.source_id)
                .and_then(|n| n.file_path.clone())
        })
        .collect();
    rdeps.sort();
    rdeps.dedup();
    if rdeps.is_empty() {
        println!("{} is not used by any indexed files.", file);
    } else {
        println!("{} is used by:", file);
        for dep in rdeps {
            println!("  - {}", dep);
        }
    }
    Ok(())
}

fn cmd_diff(repo_root: &Path, base: &str, head: &str) -> anyhow::Result<()> {
    let git = GitIntegration::new(repo_root);
    if !git.is_git_repo() {
        warn!("Not a git repository.");
        return Ok(());
    }
    let changes = git.diff_files(base, head)?;
    if changes.is_empty() {
        println!("No changes between {} and {}.", base, head);
        return Ok(());
    }
    println!("Changed files:");
    for change in &changes {
        println!(
            "  {:10} {} (+{} -{})",
            change.status, change.path, change.lines_added, change.lines_deleted
        );
    }
    // Impacted files (reverse deps)
    if let Ok(db) = CacheDb::open(&repo_root.join(CACHE_DIR).join(CACHE_DB)) {
        let graph = rebuild_graph(&db)?;
        let mut impacted = Vec::new();
        let changed_paths: Vec<&str> = changes.iter().map(|c| c.path.as_str()).collect();
        for edge in graph
            .edges
            .iter()
            .filter(|edge| edge.kind == EdgeKind::Imports)
        {
            let target_path = graph
                .nodes
                .get(&edge.target_id)
                .and_then(|n| n.file_path.as_deref());
            if let Some(tp) = target_path {
                if changed_paths.contains(&tp) {
                    if let Some(sp) = graph
                        .nodes
                        .get(&edge.source_id)
                        .and_then(|n| n.file_path.clone())
                    {
                        if !changed_paths.contains(&sp.as_str()) {
                            if !impacted.contains(&sp) {
                                impacted.push(sp);
                            }
                        }
                    }
                }
            }
        }
        if !impacted.is_empty() {
            println!("\nImpacted files:");
            for path in &impacted {
                println!("  - {}", path);
            }
        }
    }
    Ok(())
}

fn cmd_context(
    repo_root: &Path,
    reposcry_dir: &Path,
    db_path: &Path,
    task: &str,
    budget: u32,
    strict: bool,
    full: bool,
    format: &str,
    max_symbols_per_file: u32,
    show_omitted: bool,
) -> anyhow::Result<()> {
    ensure_reposcry_dir(reposcry_dir)?;
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let git = GitIntegration::new(repo_root);
    let output_fmt = match format {
        "json" => OutputFormat::Json,
        "human" => OutputFormat::Human,
        _ => OutputFormat::Markdown,
    };
    let config = ContextConfig {
        token_budget: budget,
        strict_mode: strict,
        max_files: 30,
        max_reverse_depth: 2,
        include_full_files: full,
        format: output_fmt,
        max_symbols_per_file,
        show_omitted,
    };
    let context = ContextBuilder::new(graph, config)
        .with_cache(db)
        .with_git(git)
        .build(task)?;
    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&context)?);
        }
        "human" => {
            println!("Context for: {}", context.user_task);
            println!();
            if context.relevant_files.is_empty() {
                println!("No relevant files found.");
            } else {
                println!("Relevant files:");
                for file in &context.relevant_files {
                    println!("  {}", file.path);
                    for sym in &file.important_symbols {
                        println!("    - {}", sym);
                    }
                }
            }
            println!();
            println!("Confidence: {:?}", context.confidence);
            for warning in &context.strict_warnings {
                println!("  [!] {}", warning);
            }
        }
        _ => {
            let builder = ContextBuilder::new(CodeGraph::new(), ContextConfig::default());
            println!("{}", builder.render_markdown(&context));
        }
    }
    Ok(())
}

fn cmd_report(
    repo_root: &Path,
    db_path: &Path,
    base: &str,
    head: &str,
    format: &str,
) -> anyhow::Result<()> {
    let git = GitIntegration::new(repo_root);
    if !git.is_git_repo() {
        warn!("Not a git repository.");
        return Ok(());
    }
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let rules_config = RulesConfig::default_rules();
    let rules_engine = RulesEngine::new(rules_config);
    let report = generate_report(&graph, &git, &rules_engine, base, head)?;
    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        _ => {
            println!("{}", render_markdown(&report));
        }
    }
    Ok(())
}

fn cmd_rules(_repo_root: &Path, db_path: &Path, action: Option<RulesAction>) -> anyhow::Result<()> {
    let action = action.unwrap_or(RulesAction::Check);
    match action {
        RulesAction::Check => {
            let db = CacheDb::open(db_path)?;
            let graph = rebuild_graph(&db)?;
            let rules_config = RulesConfig::default_rules();
            let rules_engine = RulesEngine::new(rules_config);
            // Check graph-level rules
            let violations = rules_engine.check_graph(&graph);
            if violations.is_empty() {
                println!("No architecture violations found.");
            } else {
                for v in &violations {
                    println!("[{}] Rule '{}': {}", v.severity.as_str(), v.rule, v.message);
                }
            }
        }
    }
    Ok(())
}

fn cmd_validate(repo_root: &Path, db_path: &Path, base: &str, head: &str) -> anyhow::Result<()> {
    let git = GitIntegration::new(repo_root);
    if !git.is_git_repo() {
        warn!("Not a git repository.");
        return Ok(());
    }
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let rules_config = RulesConfig::default_rules();
    let rules_engine = RulesEngine::new(rules_config);
    let changes = git.diff_files(base, head)?;
    let report = generate_report(&graph, &git, &rules_engine, base, head)?;
    let mut has_errors = false;
    println!("# Validation Report\n");
    if !report.new_cycles.is_empty() {
        has_errors = true;
        println!("## Dependency Cycles\n");
        for cycle in &report.new_cycles {
            println!("ERROR: New dependency cycle: {}", cycle.join(" → "));
        }
        println!();
    }
    if !report.high_risk_changes.is_empty() {
        println!("## High-Risk Changes\n");
        for item in &report.high_risk_changes {
            println!("WARNING: {} changed — {}", item.file, item.reason);
        }
        println!();
    }
    if !report.violations.is_empty() {
        has_errors = true;
        println!("## Architecture Violations\n");
        for v in &report.violations {
            println!("{}: {}", v.rule, v.message);
        }
        println!();
    }
    // Check for changed files without test changes
    let changed_paths: Vec<String> = changes.iter().map(|c| c.path.clone()).collect();
    let has_test_changes = changed_paths.iter().any(|p| p.contains("test"));
    let has_source_changes = changed_paths.iter().any(|p| !p.contains("test"));
    if has_source_changes && !has_test_changes {
        println!("WARNING: Source files changed but no test files modified.");
        println!();
    }
    if !has_errors {
        println!("Validation passed.");
    }
    Ok(())
}

fn cmd_explain(_repo_root: &Path, db_path: &Path, file: &str) -> anyhow::Result<()> {
    let db = CacheDb::open(db_path)?;
    let graph = rebuild_graph(&db)?;
    let nodes: Vec<_> = graph
        .nodes
        .values()
        .filter(|n| n.file_path.as_deref() == Some(file))
        .collect();
    if nodes.is_empty() {
        println!("No graph data for: {}", file);
        return Ok(());
    }
    println!("File: {}", file);
    // File-level info
    if let Some(cached) = db.get_file_by_path(file)? {
        println!("Language: {}", cached.language);
        println!("Size: {} bytes, {} lines", cached.size_bytes, cached.loc);
        println!("Last indexed: {}", cached.last_indexed_at);
    }
    println!();
    // Dependencies
    let deps: Vec<u64> = graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Imports)
        .filter(|e| {
            graph
                .nodes
                .get(&e.source_id)
                .and_then(|n| n.file_path.as_deref())
                == Some(file)
        })
        .map(|e| e.target_id)
        .collect();
    if !deps.is_empty() {
        println!("Dependencies:");
        for dep_id in &deps {
            if let Some(node) = graph.get_node(*dep_id) {
                if let Some(path) = &node.file_path {
                    println!("  - {}", path);
                }
            }
        }
        println!();
    }
    // Reverse dependencies
    let rdeps: Vec<u64> = graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Imports)
        .filter(|e| {
            graph
                .nodes
                .get(&e.target_id)
                .and_then(|n| n.file_path.as_deref())
                == Some(file)
        })
        .map(|e| e.source_id)
        .collect();
    if !rdeps.is_empty() {
        println!("Used by:");
        for rdep_id in &rdeps {
            if let Some(node) = graph.get_node(*rdep_id) {
                if let Some(path) = &node.file_path {
                    println!("  - {}", path);
                }
            }
        }
        println!();
    }
    // Symbols
    if let Some(cached) = db.get_file_by_path(file)? {
        let symbols = db.get_symbols_by_file(cached.id)?;
        if !symbols.is_empty() {
            println!("Symbols:");
            for sym in &symbols {
                let vis = sym.visibility.as_deref().unwrap_or("");
                println!(
                    "  {} {} {} (line {})",
                    vis, sym.kind, sym.name, sym.start_line
                );
            }
        }
    }
    Ok(())
}

fn rebuild_graph(db: &CacheDb) -> anyhow::Result<CodeGraph> {
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
        let symbols = db.get_symbols_by_file(file.id)?;
        for sym in &symbols {
            let sym_id = graph.add_node(
                match sym.kind.as_str() {
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
                },
                &sym.name,
            );
            if let Some(node) = graph.nodes.get_mut(&sym_id) {
                node.file_path = Some(file.path.clone());
                node.start_line = Some(sym.start_line);
                node.end_line = Some(sym.end_line);
                node.signature = sym.signature.clone();
                node.visibility = sym.visibility.clone();
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

#[cfg(test)]
mod tests {
    use super::*;
    use reposcry_cache::db::{CachedCallSite, CachedFile, CachedImport};
    use reposcry_graph::symbol::Symbol;

    fn file(id: i64, path: &str) -> CachedFile {
        CachedFile {
            id,
            path: path.to_string(),
            language: "typescript".to_string(),
            hash: "hash".to_string(),
            size_bytes: 1,
            loc: 1,
            last_indexed_at: "now".to_string(),
        }
    }

    #[test]
    fn resolves_relative_typescript_import() {
        let files = vec![
            file(1, "src/components/button.tsx"),
            file(2, "src/lib/theme.ts"),
        ];
        let resolved = resolve_import_target("src/components/button.tsx", "../lib/theme", &files);
        assert_eq!(resolved.as_deref(), Some("src/lib/theme.ts"));
    }

    #[test]
    fn resolves_alias_typescript_import() {
        let files = vec![file(1, "src/lib/theme.ts")];
        let resolved = resolve_import_target("src/components/button.tsx", "@/lib/theme", &files);
        assert_eq!(resolved.as_deref(), Some("src/lib/theme.ts"));
    }

    #[test]
    fn resolves_workspace_rust_import_to_module_file() {
        let files = vec![
            file(1, "crates/reposcry-graph/src/lib.rs"),
            file(2, "crates/reposcry-graph/src/edge.rs"),
        ];
        let resolved = resolve_import_target(
            "crates/reposcry-cli/src/main.rs",
            "reposcry_graph::edge::EdgeKind",
            &files,
        );
        assert_eq!(
            resolved.as_deref(),
            Some("crates/reposcry-graph/src/edge.rs")
        );
    }

    #[test]
    fn resolves_workspace_package_import_to_source_index() {
        let files = vec![
            file(1, "apps/web/lib/graph.ts"),
            file(2, "packages/shared/src/index.ts"),
            file(3, "packages/graph/src/rebuild.ts"),
        ];
        let package_root = resolve_import_target("apps/web/lib/graph.ts", "@mixed/shared", &files);
        let package_subpath =
            resolve_import_target("apps/web/lib/graph.ts", "@large/graph/rebuild", &files);
        assert_eq!(
            package_root.as_deref(),
            Some("packages/shared/src/index.ts")
        );
        assert_eq!(
            package_subpath.as_deref(),
            Some("packages/graph/src/rebuild.ts")
        );
    }

    fn symbol(id: i64, file_path: &str, name: &str, start_line: u32, end_line: u32) -> Symbol {
        Symbol {
            id: Some(id),
            file_path: file_path.to_string(),
            name: name.to_string(),
            kind: "function".to_string(),
            start_line,
            end_line,
            signature: None,
            visibility: None,
            doc_comment: None,
        }
    }

    #[test]
    fn resolves_unique_global_callee_even_if_short_name_was_inserted_twice() {
        let target = symbol(7, "lib/graph.ts", "rebuild_graph", 3, 8);
        let mut candidates = HashMap::new();
        push_global_symbol_candidate(&mut candidates, "rebuild_graph", &target);
        push_global_symbol_candidate(&mut candidates, "rebuild_graph", &target);

        let call_site = CachedCallSite {
            id: 1,
            file_id: 1,
            caller: "refreshGraph".to_string(),
            callee: "rebuild_graph".to_string(),
            line: 6,
            confidence: 0.8,
            resolution_strategy: Some("ast_ts_call".to_string()),
        };

        let resolved =
            resolve_callee_symbol(&[], &candidates, &[], "app/actions.ts", &[], &call_site);
        let (resolved_symbol, strategy) = resolved.expect("expected unique global resolution");
        assert_eq!(resolved_symbol.file_path, "lib/graph.ts");
        assert_eq!(resolved_symbol.name, "rebuild_graph");
        assert_eq!(strategy, "unique_global");
    }

    #[test]
    fn resolves_callee_using_import_target_when_global_name_is_ambiguous() {
        let ts_symbol = symbol(7, "packages/shared/src/index.ts", "rebuild_graph", 5, 7);
        let rust_symbol = symbol(8, "crates/worker/src/graph.rs", "rebuild_graph", 1, 3);
        let mut candidates = HashMap::new();
        push_global_symbol_candidate(&mut candidates, "rebuild_graph", &ts_symbol);
        push_global_symbol_candidate(&mut candidates, "rebuild_graph", &rust_symbol);

        let imports = vec![CachedImport {
            id: 1,
            file_id: 2,
            source: "apps/web/lib/graph.ts".to_string(),
            target: "@mixed/shared".to_string(),
            is_relative: false,
            imported_names: vec!["read_cache".to_string(), "rebuild_graph".to_string()],
            line: 1,
        }];
        let files = vec![
            file(1, "apps/web/lib/graph.ts"),
            file(2, "packages/shared/src/index.ts"),
            file(3, "crates/worker/src/graph.rs"),
        ];
        let call_site = CachedCallSite {
            id: 1,
            file_id: 1,
            caller: "rebuildGraphView".to_string(),
            callee: "rebuild_graph".to_string(),
            line: 4,
            confidence: 0.8,
            resolution_strategy: Some("ast_ts_call".to_string()),
        };

        let resolved = resolve_callee_symbol(
            &[],
            &candidates,
            &imports,
            "apps/web/lib/graph.ts",
            &files,
            &call_site,
        );
        let (resolved_symbol, strategy) = resolved.expect("expected import-based resolution");
        assert_eq!(resolved_symbol.file_path, "packages/shared/src/index.ts");
        assert_eq!(resolved_symbol.name, "rebuild_graph");
        assert_eq!(strategy, "import_resolved");
    }
}
