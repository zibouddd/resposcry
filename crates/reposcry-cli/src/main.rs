use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use clap::{Parser, Subcommand};

mod installer;

use installer::{install_platform, InstallOptions, InstallPlatform};
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;

use reposcry_cache::db::CacheDb;
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
    Symbols {
        file: String,
    },
    /// Show file dependencies
    Deps {
        file: String,
    },
    /// Show reverse dependencies
    Rdeps {
        file: String,
    },
    /// Show diff impact
    Diff {
        base: String,
        #[arg(default_value = "HEAD")]
        head: String,
    },
    /// Generate AI context pack
    Context {
        task: String,
        /// Token budget
        #[arg(long, default_value = "20000")]
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
    Explain {
        file: String,
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
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
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
        Commands::Index { preset } => {
            cmd_index(&repo_root, &reposcry_dir, &db_path, preset.as_deref())
        }
        Commands::Stats => cmd_stats(&db_path),
        Commands::Files { language } => {
            cmd_files(&repo_root, &db_path, language)
        }
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
        } => cmd_context(
            &repo_root,
            &reposcry_dir,
            &db_path,
            &task,
            budget,
            strict,
            full,
            &format,
        ),
        Commands::Report {
            base,
            head,
            format,
        } => cmd_report(&repo_root, &db_path, &base, &head, &format),
        Commands::Rules { action } => {
            cmd_rules(&repo_root, &db_path, action)
        }
        Commands::Validate { base, head } => {
            cmd_validate(&repo_root, &db_path, &base, &head)
        }
        Commands::Explain { file } => {
            cmd_explain(&repo_root, &db_path, &file)
        }
    }
}

fn ensure_reposcry_dir(reposcry_dir: &Path) -> anyhow::Result<()> {
    if !reposcry_dir.exists() {
        std::fs::create_dir_all(reposcry_dir)?;
    }
    Ok(())
}

fn cmd_init(
    repo_root: &Path,
    reposcry_dir: &Path,
    db_path: &Path,
) -> anyhow::Result<()> {
    info!("Initializing RepoScry in {}", repo_root.display());
    ensure_reposcry_dir(reposcry_dir)?;
    // Open database to initialize schema
    let _db = CacheDb::open(db_path)?;
    _db.set_config("repo_root", &repo_root.to_string_lossy())?;
    _db.set_config("version", env!("CARGO_PKG_VERSION"))?;
    _db.set_config(
        "initialized_at",
        &chrono::Utc::now().to_rfc3339(),
    )?;
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
    let summary = install_platform(
        repo_root,
        platform,
        InstallOptions { force, dry_run },
    )?;
    println!(
        "Installed RepoScry integration for {}",
        platform.label()
    );
    if dry_run {
        println!("Dry run only. No files were written.");
    }
    for write in &summary.writes {
        println!("  {:16} {}", write.action, write.path.display());
    }
    println!();
    println!("Next commands:");
    println!("  reposcry index");
    println!("  reposcry context \"your task\" --strict --budget 20000");
    println!("  reposcry validate main...HEAD");
    Ok(())
}

fn cmd_index(
    repo_root: &Path,
    reposcry_dir: &Path,
    db_path: &Path,
    preset_name: Option<&str>,
) -> anyhow::Result<()> {
    info!("Indexing repository: {}", repo_root.display());
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
    let files = scanner.scan()?;
    info!("Found {} files to index", files.len());
    let mut parsed_count = 0usize;
    let mut skipped_count = 0usize;
    for file in &files {
        let source = match std::fs::read_to_string(&file.path) {
            Ok(s) => s,
            Err(e) => {
                warn!(
                    "Skipping unreadable file {}: {}",
                    file.relative_path, e
                );
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
                    parsed_count += 1;
                    debug!(
                        "Indexed: {} ({} symbols, {} imports)",
                        file.relative_path,
                        parsed.symbols.len(),
                        parsed.imports.len()
                    );
                }
                Err(e) => {
                    warn!(
                        "Failed to parse {}: {}",
                        file.relative_path, e
                    );
                    let db_file_id = db.upsert_file(
                        &file.relative_path,
                        &file.language,
                        &hash,
                        file.size_bytes as i64,
                        0,
                    )?;
                    db.insert_symbols(db_file_id, &[])?;
                    db.insert_imports(db_file_id, &[])?;
                }
            }
        } else {
            skipped_count += 1;
            debug!("Skipping unchanged: {}", file.relative_path);
        }
    }
    rebuild_persisted_import_edges(&db)?;
    let preset_name = preset_name.unwrap_or("none");
    db.set_config(
        "last_indexed_at",
        &chrono::Utc::now().to_rfc3339(),
    )?;
    db.set_config("preset", preset_name)?;
    db.set_config("files_count", &files.len().to_string())?;
    info!(
        "Indexing complete. {} files indexed, {} parsed, {} unchanged, {} imports, {} edges.",
        db.file_count()?,
        parsed_count,
        skipped_count,
        db.import_count()?,
        db.edge_count()?
    );
    Ok(())
}

fn rebuild_persisted_import_edges(db: &CacheDb) -> anyhow::Result<()> {
    let files = db.get_all_files()?;
    db.clear_edges_by_kind(EdgeKind::Imports)?;
    for file in &files {
        let imports = db.get_imports_by_file(file.id)?;
        for import in imports {
            if let Some(target_path) =
                resolve_import_target(&file.path, &import.target, &files)
            {
                if let Some(target_file) =
                    files.iter().find(|candidate| candidate.path == target_path)
                {
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

fn resolve_import_target(
    source_path: &str,
    raw_target: &str,
    files: &[reposcry_cache::db::CachedFile],
) -> Option<String> {
    let raw_target = raw_target
        .trim()
        .trim_matches(';')
        .trim_matches('"')
        .trim_matches('\'');
    if raw_target.is_empty() {
        return None;
    }
    let file_paths: HashSet<&str> =
        files.iter().map(|file| file.path.as_str()).collect();

    if raw_target.starts_with('.') {
        let parent = Path::new(source_path)
            .parent()
            .unwrap_or_else(|| Path::new(""));
        let base = normalize_relative_path(parent.join(raw_target));
        return find_candidate_path(&base, &file_paths);
    }
    if let Some(rest) = raw_target.strip_prefix("@/") {
        let candidates = [
            find_candidate_path(rest, &file_paths),
            find_candidate_path(&format!("src/{}", rest), &file_paths),
        ];
        return candidates.into_iter().flatten().next();
    }
    if let Some(rest) = raw_target.strip_prefix("~/") {
        return find_candidate_path(rest, &file_paths);
    }
    resolve_rust_import_target(source_path, raw_target, &file_paths)
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

fn find_candidate_path(
    base: &str,
    file_paths: &HashSet<&str>,
) -> Option<String> {
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
            Component::Normal(part) => {
                parts.push(part.to_string_lossy().to_string())
            }
            _ => {}
        }
    }
    parts.join("/")
}

fn normalize_slashes(path: &str) -> String {
    path.replace('\\', "/")
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

fn cmd_files(
    _repo_root: &Path,
    db_path: &Path,
    language: Option<String>,
) -> anyhow::Result<()> {
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

fn cmd_deps(
    _repo_root: &Path,
    db_path: &Path,
    file: &str,
) -> anyhow::Result<()> {
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

fn cmd_rdeps(
    _repo_root: &Path,
    db_path: &Path,
    file: &str,
) -> anyhow::Result<()> {
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
    if let Ok(db) =
        CacheDb::open(&repo_root.join(CACHE_DIR).join(CACHE_DB))
    {
        let graph = rebuild_graph(&db)?;
        let mut impacted = Vec::new();
        let changed_paths: Vec<&str> =
            changes.iter().map(|c| c.path.as_str()).collect();
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
                println!("  ⚠ {}", warning);
            }
        }
        _ => {
            let builder = ContextBuilder::new(
                CodeGraph::new(),
                ContextConfig::default(),
            );
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

fn cmd_rules(
    _repo_root: &Path,
    db_path: &Path,
    action: Option<RulesAction>,
) -> anyhow::Result<()> {
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
                    println!(
                        "[{}] Rule '{}': {}",
                        v.severity.as_str(),
                        v.rule,
                        v.message
                    );
                }
            }
        }
    }
    Ok(())
}

fn cmd_validate(
    repo_root: &Path,
    db_path: &Path,
    base: &str,
    head: &str,
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
    let changes = git.diff_files(base, head)?;
    let report =
        generate_report(&graph, &git, &rules_engine, base, head)?;
    let mut has_errors = false;
    println!("# Validation Report\n");
    if !report.new_cycles.is_empty() {
        has_errors = true;
        println!("## Dependency Cycles\n");
        for cycle in &report.new_cycles {
            println!(
                "ERROR: New dependency cycle: {}",
                cycle.join(" → ")
            );
        }
        println!();
    }
    if !report.high_risk_changes.is_empty() {
        println!("## High-Risk Changes\n");
        for item in &report.high_risk_changes {
            println!(
                "WARNING: {} changed — {}",
                item.file, item.reason
            );
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
    let changed_paths: Vec<String> =
        changes.iter().map(|c| c.path.clone()).collect();
    let has_test_changes = changed_paths.iter().any(|p| p.contains("test"));
    let has_source_changes =
        changed_paths.iter().any(|p| !p.contains("test"));
    if has_source_changes && !has_test_changes {
        println!(
            "WARNING: Source files changed but no test files modified."
        );
        println!();
    }
    if !has_errors {
        println!("Validation passed.");
    }
    Ok(())
}

fn cmd_explain(
    _repo_root: &Path,
    db_path: &Path,
    file: &str,
) -> anyhow::Result<()> {
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
        println!(
            "Size: {} bytes, {} lines",
            cached.size_bytes, cached.loc
        );
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
        let Some(&source_graph_id) =
            db_file_to_graph_node.get(&cached_edge.source_file_id)
        else {
            continue;
        };
        let Some(target_file_id) = cached_edge.target_file_id else {
            continue;
        };
        let Some(&target_graph_id) =
            db_file_to_graph_node.get(&target_file_id)
        else {
            continue;
        };
        graph.add_edge(
            source_graph_id,
            target_graph_id,
            EdgeKind::Imports,
        );
    }
    Ok(graph)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reposcry_cache::db::CachedFile;

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
        let resolved = resolve_import_target(
            "src/components/button.tsx",
            "../lib/theme",
            &files,
        );
        assert_eq!(resolved.as_deref(), Some("src/lib/theme.ts"));
    }

    #[test]
    fn resolves_alias_typescript_import() {
        let files = vec![file(1, "src/lib/theme.ts")];
        let resolved = resolve_import_target(
            "src/components/button.tsx",
            "@/lib/theme",
            &files,
        );
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
}
