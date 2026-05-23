use std::path::PathBuf;
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use reposcry_cache::db::CacheDb;
use serde::Serialize;

const CACHE_DIR: &str = ".reposcry";
const CACHE_DB: &str = "reposcry.db";

#[derive(Parser)]
#[command(
    name = "reposcry-index-full",
    version,
    about = "Run a full RepoScry indexing pass and emit a JSON summary"
)]
struct Cli {
    #[arg(global = true, long = "repo", short = 'C', default_value = ".")]
    repo_root: String,

    #[arg(long)]
    preset: Option<String>,
}

#[derive(Debug, Serialize)]
struct StepResult {
    name: String,
    ok: bool,
    detail: String,
}

#[derive(Debug, Serialize)]
struct IndexSummary {
    files: i64,
    symbols: i64,
    imports: i64,
    edges: i64,
    preset: Option<String>,
}

#[derive(Debug, Serialize)]
struct Output {
    tool: String,
    repo: String,
    steps: Vec<StepResult>,
    summary: IndexSummary,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo_root = PathBuf::from(&cli.repo_root).canonicalize()?;

    let reposcry_path = sibling_reposcry_exe()?;
    let mut index_command = Command::new(&reposcry_path);
    index_command.arg("--repo").arg(&repo_root).arg("index");
    if let Some(preset) = &cli.preset {
        index_command.arg("--preset").arg(preset);
    }

    let index_status = index_command
        .status()
        .with_context(|| format!("failed to launch {}", reposcry_path.display()))?;
    if !index_status.success() {
        return Err(anyhow!(
            "reposcry index failed with status {}",
            index_status
        ));
    }

    let db_path = repo_root.join(CACHE_DIR).join(CACHE_DB);
    let db = CacheDb::open(&db_path)?;
    let output = Output {
        tool: "reposcry-index-full".to_string(),
        repo: repo_root.display().to_string(),
        steps: vec![StepResult {
            name: "index".to_string(),
            ok: true,
            detail: "reposcry index completed successfully".to_string(),
        }],
        summary: IndexSummary {
            files: db.file_count()?,
            symbols: db.symbol_count()?,
            imports: db.import_count()?,
            edges: db.edge_count()?,
            preset: cli.preset.or_else(|| db.get_config("preset").ok().flatten()),
        },
    };

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn sibling_reposcry_exe() -> Result<PathBuf> {
    let current = std::env::current_exe()?;
    let parent = current
        .parent()
        .ok_or_else(|| anyhow!("failed to resolve executable directory"))?;
    let filename = if cfg!(windows) {
        "reposcry.exe"
    } else {
        "reposcry"
    };
    let path = parent.join(filename);
    if path.exists() {
        Ok(path)
    } else {
        Err(anyhow!(
            "could not find sibling reposcry executable at {}",
            path.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn cache_db_path_constants_are_stable() {
        assert_eq!(Path::new(CACHE_DIR).join(CACHE_DB), PathBuf::from(".reposcry").join("reposcry.db"));
    }
}
