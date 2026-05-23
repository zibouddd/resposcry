use std::collections::{HashSet, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use clap::Parser;
use reposcry_cache::db::{CacheDb, CachedFile};
use reposcry_graph::edge::EdgeKind;
use reposcry_graph::language::Language;
use reposcry_indexer::parser::parse_file;
use serde::Serialize;

const CACHE_DIR: &str = ".reposcry";
const CACHE_DB: &str = "reposcry.db";

#[derive(Parser)]
#[command(
    name = "reposcry-update",
    version,
    about = "Incrementally update RepoScry cache for changed files"
)]
struct Cli {
    #[arg(long = "repo", short = 'C', default_value = ".")]
    repo_root: String,

    /// Update files changed in git status / diff.
    #[arg(long, default_value_t = false)]
    changed: bool,

    /// Explicit files to update. Can be combined with --changed.
    #[arg(long = "file")]
    files: Vec<String>,

    /// Base ref for diff detection. Defaults to HEAD; use main for branch work.
    #[arg(long, default_value = "HEAD")]
    base: String,

    /// Skip rebuilding persisted call edges after file updates.
    #[arg(long, default_value_t = false)]
    skip_warm_calls: bool,

    /// Also rebuild lexical search documents. Semantic vectors are not rebuilt.
    #[arg(long, default_value_t = false)]
    refresh_search: bool,
}

#[derive(Debug, Serialize)]
struct UpdateReport {
    repo: String,
    changed_mode: bool,
    base: String,
    files_requested: usize,
    files_seen: usize,
    parsed: Vec<String>,
    deleted: Vec<String>,
    skipped: Vec<String>,
    errors: Vec<String>,
    rebuilt_import_edges: usize,
    warm_calls_ran: bool,
    refresh_search_ran: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo_root = PathBuf::from(&cli.repo_root).canonicalize()?;
    let db_path = repo_root.join(CACHE_DIR).join(CACHE_DB);
    let db = CacheDb::open(&db_path)?;

    let mut paths = BTreeSet::<String>::new();
    for file in &cli.files {
        paths.insert(normalize_slashes(file));
    }
    if cli.changed {
        for file in git_changed_files(&repo_root, &cli.base)? {
            paths.insert(file);
        }
    }
    if !cli.changed && cli.files.is_empty() {
        anyhow::bail!("nothing to update: pass --changed or --file <path>");
    }

    let mut report = UpdateReport {
        repo: repo_root.display().to_string(),
        changed_mode: cli.changed,
        base: cli.base.clone(),
        files_requested: cli.files.len(),
        files_seen: paths.len(),
        parsed: Vec::new(),
        deleted: Vec::new(),
        skipped: Vec::new(),
        errors: Vec::new(),
        rebuilt_import_edges: 0,
        warm_calls_ran: false,
        refresh_search_ran: false,
    };

    for relative_path in paths {
        if should_skip_path(&relative_path) {
            report.skipped.push(relative_path);
            continue;
        }
        let absolute_path = repo_root.join(&relative_path);
        if !absolute_path.exists() {
            match db.delete_file(&relative_path) {
                Ok(()) => report.deleted.push(relative_path),
                Err(error) => report.errors.push(format!("{}: {}", relative_path, error)),
            }
            continue;
        }
        if !absolute_path.is_file() {
            report.skipped.push(relative_path);
            continue;
        }
        let language = Language::from_extension(&relative_path);
        if matches!(language, Language::Unknown(_)) {
            report.skipped.push(relative_path);
            continue;
        }
        match update_one_file(&db, &relative_path, &absolute_path, language) {
            Ok(updated) => {
                if updated {
                    report.parsed.push(relative_path);
                } else {
                    report.skipped.push(relative_path);
                }
            }
            Err(error) => report.errors.push(format!("{}: {}", relative_path, error)),
        }
    }

    report.rebuilt_import_edges = rebuild_persisted_import_edges(&db)?;

    if !cli.skip_warm_calls {
        match run_reposcry(&repo_root, &["warm-calls"]) {
            Ok(()) => report.warm_calls_ran = true,
            Err(error) => report.errors.push(format!("warm-calls: {}", error)),
        }
    }

    if cli.refresh_search {
        match run_reposcry(&repo_root, &["refresh-search", "--no-semantic"]) {
            Ok(()) => report.refresh_search_ran = true,
            Err(error) => report.errors.push(format!("refresh-search: {}", error)),
        }
    }

    println!("{}", serde_json::to_string_pretty(&report)?);
    if !report.errors.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}

fn update_one_file(
    db: &CacheDb,
    relative_path: &str,
    absolute_path: &Path,
    language: Language,
) -> Result<bool> {
    let source = std::fs::read_to_string(absolute_path)
        .with_context(|| format!("failed to read {}", absolute_path.display()))?;
    let hash = blake3::hash(source.as_bytes()).to_hex().to_string();
    let existing = db.get_file_by_path(relative_path)?;
    let needs_parse = match existing.as_ref() {
        Some(file) => file.hash != hash || db.get_call_sites_by_file(file.id)?.is_empty(),
        None => true,
    };
    if !needs_parse {
        return Ok(false);
    }

    let parsed = parse_file(relative_path, &source)?;
    let file_id = db.upsert_file(
        relative_path,
        language.as_str(),
        &hash,
        source.len() as i64,
        parsed.loc as i64,
    )?;
    db.insert_symbols(file_id, &parsed.symbols)?;
    db.insert_imports(file_id, &parsed.imports)?;
    db.insert_call_sites(file_id, &parsed.calls)?;
    Ok(true)
}

fn git_changed_files(repo_root: &Path, base: &str) -> Result<Vec<String>> {
    let mut paths = BTreeSet::<String>::new();

    let status = Command::new("git")
        .current_dir(repo_root)
        .args(["status", "--porcelain"])
        .output()
        .context("failed to run git status")?;
    if status.status.success() {
        let stdout = String::from_utf8_lossy(&status.stdout);
        for line in stdout.lines() {
            if line.len() < 4 {
                continue;
            }
            let path_part = line[3..].trim();
            let path = path_part
                .split(" -> ")
                .last()
                .unwrap_or(path_part)
                .trim();
            if !path.is_empty() {
                paths.insert(normalize_slashes(path));
            }
        }
    }

    let diff = Command::new("git")
        .current_dir(repo_root)
        .args(["diff", "--name-only", base])
        .output()
        .context("failed to run git diff")?;
    if diff.status.success() {
        let stdout = String::from_utf8_lossy(&diff.stdout);
        for line in stdout.lines() {
            let line = line.trim();
            if !line.is_empty() {
                paths.insert(normalize_slashes(line));
            }
        }
    }

    Ok(paths.into_iter().collect())
}

fn rebuild_persisted_import_edges(db: &CacheDb) -> Result<usize> {
    let files = db.get_all_files()?;
    db.clear_edges_by_kind(EdgeKind::Imports)?;
    let mut inserted = 0usize;
    for file in &files {
        let imports = db.get_imports_by_file(file.id)?;
        for import in imports {
            if let Some(target_path) = resolve_import_target(&file.path, &import.target, &files) {
                if let Some(target_file) = files.iter().find(|candidate| candidate.path == target_path) {
                    db.insert_edge(
                        file.id,
                        Some(target_file.id),
                        Some(&target_path),
                        EdgeKind::Imports,
                        1.0,
                    )?;
                    inserted += 1;
                }
            }
        }
    }
    Ok(inserted)
}

fn resolve_import_target(source_path: &str, raw_target: &str, files: &[CachedFile]) -> Option<String> {
    let raw_target = raw_target
        .trim()
        .trim_matches(';')
        .trim_matches('"')
        .trim_matches('\'');
    if raw_target.is_empty() {
        return None;
    }
    let file_paths: HashSet<&str> = files.iter().map(|file| file.path.as_str()).collect();

    if raw_target.starts_with('.') {
        let parent = Path::new(source_path).parent().unwrap_or_else(|| Path::new(""));
        let base = normalize_relative_path(parent.join(raw_target));
        return find_candidate_path(&base, &file_paths);
    }
    if let Some(rest) = raw_target.strip_prefix("@/") {
        return find_candidate_path(rest, &file_paths)
            .or_else(|| find_candidate_path(&format!("src/{}", rest), &file_paths));
    }
    if let Some(rest) = raw_target.strip_prefix("~/") {
        return find_candidate_path(rest, &file_paths);
    }
    if let Some(resolved) = resolve_workspace_package_import_target(raw_target, &file_paths) {
        return Some(resolved);
    }
    resolve_rust_import_target(source_path, raw_target, &file_paths)
}

fn resolve_workspace_package_import_target(raw_target: &str, file_paths: &HashSet<&str>) -> Option<String> {
    let segments: Vec<&str> = raw_target.split('/').filter(|segment| !segment.is_empty()).collect();
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
            if let Some(resolved) = find_candidate_path(&format!("{}/src/index", root), file_paths) {
                return Some(resolved);
            }
            if let Some(resolved) = find_candidate_path(&format!("{}/index", root), file_paths) {
                return Some(resolved);
            }
            continue;
        }
        let subpath = subpath_segments.join("/");
        if let Some(resolved) = find_candidate_path(&format!("{}/src/{}", root, subpath), file_paths) {
            return Some(resolved);
        }
        if let Some(resolved) = find_candidate_path(&format!("{}/{}", root, subpath), file_paths) {
            return Some(resolved);
        }
    }
    None
}

fn resolve_rust_import_target(source_path: &str, raw_target: &str, file_paths: &HashSet<&str>) -> Option<String> {
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
            let segments: Vec<&str> = rest.split("::").filter(|segment| !segment.is_empty()).collect();
            return find_rust_module_candidate(&src_root, &segments, file_paths);
        }
    }
    if let Some(rest) = cleaned.strip_prefix("self::") {
        let root = normalize_relative_path(source_parent.to_path_buf());
        let segments: Vec<&str> = rest.split("::").filter(|segment| !segment.is_empty()).collect();
        return find_rust_module_candidate(&root, &segments, file_paths);
    }
    if let Some(rest) = cleaned.strip_prefix("super::") {
        let parent = source_parent.parent().unwrap_or_else(|| Path::new(""));
        let root = normalize_relative_path(parent.to_path_buf());
        let segments: Vec<&str> = rest.split("::").filter(|segment| !segment.is_empty()).collect();
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

fn find_rust_module_candidate(root: &str, segments: &[&str], file_paths: &HashSet<&str>) -> Option<String> {
    for len in (1..=segments.len()).rev() {
        let base = format!("{}/{}", root.trim_end_matches('/'), segments[..len].join("/"));
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
    candidate_paths(&normalized)
        .into_iter()
        .find(|candidate| file_paths.contains(candidate.as_str()))
}

fn candidate_paths(base: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let base = base.trim_end_matches('/');
    if !base.is_empty() {
        candidates.push(base.to_string());
    }
    let extensions = ["ts", "tsx", "js", "jsx", "rs", "py", "json", "toml", "yaml", "yml"];
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

fn should_skip_path(path: &str) -> bool {
    let path = normalize_slashes(path);
    let lower = path.to_lowercase();
    lower.starts_with(".git/")
        || lower.starts_with(".reposcry/")
        || lower.contains("/node_modules/")
        || lower.starts_with("node_modules/")
        || lower.contains("/target/")
        || lower.starts_with("target/")
        || lower.contains("/.next/")
        || lower.starts_with(".next/")
        || lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".webp")
        || lower.ends_with(".gif")
        || lower.ends_with(".mp4")
        || lower.ends_with(".zip")
        || lower.ends_with(".pdf")
        || lower.ends_with(".lock")
}

fn run_reposcry(repo_root: &Path, args: &[&str]) -> Result<()> {
    let mut command_args = vec!["--repo".to_string(), repo_root.display().to_string()];
    command_args.extend(args.iter().map(|arg| arg.to_string()));
    let status = Command::new("reposcry").args(&command_args).status()?;
    if !status.success() {
        anyhow::bail!("reposcry {} exited with {}", args.join(" "), status);
    }
    Ok(())
}
