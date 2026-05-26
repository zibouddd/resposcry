use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use serde::Serialize;

#[derive(Parser)]
#[command(
    name = "reposcry-watch",
    version,
    about = "Watch a repository and incrementally update RepoScry when files change"
)]
struct Cli {
    #[arg(long = "repo", short = 'C', default_value = ".")]
    repo_root: String,

    /// Base ref used by reposcry-update --changed.
    #[arg(long, default_value = "HEAD")]
    base: String,

    /// Poll interval in milliseconds.
    #[arg(long, default_value_t = 1500)]
    interval_ms: u64,

    /// Also rebuild lexical search documents after changed-file updates.
    #[arg(long, default_value_t = false)]
    refresh_search: bool,

    /// Skip persisted call-edge warmup for fastest feedback.
    #[arg(long, default_value_t = false)]
    skip_warm_calls: bool,

    /// Run one polling iteration and exit. Useful in CI and editor hooks.
    #[arg(long, default_value_t = false)]
    once: bool,

    /// Print JSON events instead of human text.
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Debug, Serialize)]
struct WatchEvent {
    kind: String,
    repo: String,
    base: String,
    files: Vec<String>,
    ok: bool,
    elapsed_ms: u128,
    message: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo_root = PathBuf::from(&cli.repo_root).canonicalize()?;
    let mut previous = BTreeSet::<String>::new();

    loop {
        let changed = changed_files(&repo_root, &cli.base)?;
        if changed != previous && !changed.is_empty() {
            let started = Instant::now();
            let result = run_update(
                &repo_root,
                &cli.base,
                cli.refresh_search,
                cli.skip_warm_calls,
            );
            let event = WatchEvent {
                kind: "update".to_string(),
                repo: repo_root.display().to_string(),
                base: cli.base.clone(),
                files: changed.iter().cloned().collect(),
                ok: result.is_ok(),
                elapsed_ms: started.elapsed().as_millis(),
                message: result
                    .map(|_| "reposcry-update completed".to_string())
                    .unwrap_or_else(|error| error.to_string()),
            };
            emit(&event, cli.json)?;
            previous = changed;
        }

        if cli.once {
            if previous.is_empty() {
                let event = WatchEvent {
                    kind: "idle".to_string(),
                    repo: repo_root.display().to_string(),
                    base: cli.base.clone(),
                    files: Vec::new(),
                    ok: true,
                    elapsed_ms: 0,
                    message: "no changed files detected".to_string(),
                };
                emit(&event, cli.json)?;
            }
            break;
        }

        thread::sleep(Duration::from_millis(cli.interval_ms));
    }

    Ok(())
}

fn emit(event: &WatchEvent, json_output: bool) -> Result<()> {
    if json_output {
        println!("{}", serde_json::to_string(event)?);
    } else if event.files.is_empty() {
        println!("reposcry-watch: {}", event.message);
    } else {
        println!(
            "reposcry-watch: {} changed file(s), ok={}, {}ms — {}",
            event.files.len(),
            event.ok,
            event.elapsed_ms,
            event.message
        );
    }
    Ok(())
}

fn changed_files(repo_root: &Path, base: &str) -> Result<BTreeSet<String>> {
    let mut paths = BTreeSet::new();

    let status = Command::new("git")
        .current_dir(repo_root)
        .args(["status", "--porcelain"])
        .output()
        .context("failed to run git status")?;
    if status.status.success() {
        for line in String::from_utf8_lossy(&status.stdout).lines() {
            if line.len() < 4 {
                continue;
            }
            let path_part = line[3..].trim();
            let path = path_part.split(" -> ").last().unwrap_or(path_part).trim();
            if !path.is_empty() && !should_skip_path(path) {
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
        for line in String::from_utf8_lossy(&diff.stdout).lines() {
            let line = line.trim();
            if !line.is_empty() && !should_skip_path(line) {
                paths.insert(normalize_slashes(line));
            }
        }
    }

    Ok(paths)
}

fn run_update(
    repo_root: &Path,
    base: &str,
    refresh_search: bool,
    skip_warm_calls: bool,
) -> Result<()> {
    let mut args = vec![
        "--repo".to_string(),
        repo_root.display().to_string(),
        "--changed".to_string(),
        "--base".to_string(),
        base.to_string(),
    ];
    if refresh_search {
        args.push("--refresh-search".to_string());
    }
    if skip_warm_calls {
        args.push("--skip-warm-calls".to_string());
    }

    let binary = sibling_binary("reposcry-update");
    let status = Command::new(&binary)
        .args(&args)
        .status()
        .with_context(|| format!("failed to run {}", binary.display()))?;
    if !status.success() {
        anyhow::bail!("reposcry-update exited with {}", status);
    }
    Ok(())
}

fn sibling_binary(name: &str) -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from(name));
    let suffix = std::env::consts::EXE_SUFFIX;
    let binary = if suffix.is_empty() {
        name.to_string()
    } else {
        format!("{}{}", name, suffix)
    };
    exe.parent()
        .map(|parent| parent.join(binary))
        .unwrap_or_else(|| PathBuf::from(name))
}

fn normalize_slashes(path: &str) -> String {
    path.replace('\\', "/")
}

fn should_skip_path(path: &str) -> bool {
    let lower = normalize_slashes(path).to_lowercase();
    lower.starts_with(".git/")
        || lower.starts_with(".reposcry/")
        || lower.starts_with("target/")
        || lower.contains("/target/")
        || lower.starts_with("node_modules/")
        || lower.contains("/node_modules/")
        || lower.starts_with(".next/")
        || lower.contains("/.next/")
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
