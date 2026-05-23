use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitChange {
    pub path: String,
    pub status: String,
    pub lines_added: i64,
    pub lines_deleted: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommit {
    pub hash: String,
    pub author: String,
    pub date: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitBlameLine {
    pub line: u32,
    pub commit_hash: String,
    pub author: String,
    pub date: String,
}

pub struct GitIntegration {
    repo_root: PathBuf,
}

impl GitIntegration {
    pub fn new(repo_root: &Path) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
        }
    }

    fn run_git(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("git")
            .current_dir(&self.repo_root)
            .args(args)
            .output()
            .context("Failed to execute git command")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Git error: {}", stderr));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    pub fn diff_files(&self, base: &str, head: &str) -> Result<Vec<GitChange>> {
        let output = self.run_git(&["diff", "--name-status", &format!("{}...{}", base, head)])?;
        let mut changes = Vec::new();
        for line in output.lines() {
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 {
                changes.push(GitChange {
                    status: parts[0].to_string(),
                    path: parts[1].to_string(),
                    lines_added: 0,
                    lines_deleted: 0,
                });
            }
        }
        let numstat = self.run_git(&["diff", "--numstat", &format!("{}...{}", base, head)])?;
        for line in numstat.lines() {
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let added: i64 = parts[0].parse().unwrap_or(0);
                let deleted: i64 = parts[1].parse().unwrap_or(0);
                let path = parts[2];
                if let Some(change) = changes.iter_mut().find(|c| c.path == path) {
                    change.lines_added = added;
                    change.lines_deleted = deleted;
                }
            }
        }
        Ok(changes)
    }

    pub fn log(&self, since: &str, max_count: u32) -> Result<Vec<GitCommit>> {
        let output = self.run_git(&[
            "log",
            &format!("--since={}", since),
            &format!("--max-count={}", max_count),
            "--format=%H|%an|%ai|%s",
        ])?;
        let mut commits = Vec::new();
        for line in output.lines() {
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.splitn(4, '|').collect();
            if parts.len() >= 4 {
                commits.push(GitCommit {
                    hash: parts[0].to_string(),
                    author: parts[1].to_string(),
                    date: parts[2].to_string(),
                    message: parts[3].to_string(),
                });
            }
        }
        Ok(commits)
    }

    pub fn blame(&self, path: &str) -> Result<Vec<GitBlameLine>> {
        let simple = self.run_git(&["blame", "--porcelain", path])?;
        let mut blame_lines = Vec::new();
        let mut current_hash = String::new();
        let mut current_author = String::new();
        let mut current_date = String::new();
        let mut line_num: u32 = 0;
        for line in simple.lines() {
            if line.starts_with('\t') {
                line_num += 1;
                blame_lines.push(GitBlameLine {
                    line: line_num,
                    commit_hash: current_hash.clone(),
                    author: current_author.clone(),
                    date: current_date.clone(),
                });
            } else if line.len() >= 40 && line.chars().take(40).all(|c| c.is_ascii_hexdigit()) {
                current_hash = line.split(' ').next().unwrap_or("").to_string();
            } else if let Some(author) = line.strip_prefix("author ") {
                current_author = author.to_string();
            } else if let Some(date) = line.strip_prefix("author-time ") {
                current_date = date.to_string();
            }
        }
        Ok(blame_lines)
    }

    pub fn changed_files_since(&self, base: &str) -> Result<Vec<String>> {
        let output = self.run_git(&["diff", "--name-only", &format!("{}...HEAD", base)])?;
        Ok(output
            .lines()
            .map(|l| l.to_string())
            .filter(|l| !l.is_empty())
            .collect())
    }

    pub fn churn_since(&self, since: &str) -> Result<HashMap<String, u32>> {
        let output = self.run_git(&[
            "log",
            &format!("--since={}", since),
            "--name-only",
            "--format=",
            "--diff-filter=AM",
        ])?;
        let mut churn: HashMap<String, u32> = HashMap::new();
        for line in output.lines() {
            if !line.is_empty() {
                *churn.entry(line.to_string()).or_insert(0) += 1;
            }
        }
        Ok(churn)
    }

    pub fn file_owner(&self, path: &str) -> Result<String> {
        let output = self.run_git(&["shortlog", "-sn", "--", path])?;
        output
            .lines()
            .next()
            .map(|l| {
                let parts: Vec<&str> = l.split('\t').collect();
                if parts.len() > 1 {
                    parts[1].to_string()
                } else {
                    "unknown".to_string()
                }
            })
            .ok_or_else(|| anyhow::anyhow!("No owner found for {}", path))
    }

    pub fn is_git_repo(&self) -> bool {
        self.repo_root.join(".git").exists()
    }
}
