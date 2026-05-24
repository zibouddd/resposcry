use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
#[command(name = "reposcry-mcp-plus", version, about = "Expanded RepoScry MCP entrypoint")]
struct Cli {
    #[arg(long = "repo", short = 'C', default_value = ".")]
    repo_root: String,

    #[arg(long, default_value_t = 1_048_576)]
    max_request_bytes: usize,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo_root = PathBuf::from(&cli.repo_root).canonicalize()?;
    let status = sibling_binary("reposcry")
        .args([
            "--repo",
            &repo_root.display().to_string(),
            "mcp",
            "--max-request-bytes",
            &cli.max_request_bytes.to_string(),
        ])
        .status()?;
    if !status.success() {
        anyhow::bail!("reposcry mcp exited with {}", status);
    }
    Ok(())
}

fn sibling_binary(name: &str) -> Command {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from(name));
    let suffix = std::env::consts::EXE_SUFFIX;
    let binary = if suffix.is_empty() { name.to_string() } else { format!("{}{}", name, suffix) };
    let path = exe.parent().map(|parent| parent.join(binary)).unwrap_or_else(|| PathBuf::from(name));
    Command::new(path)
}
