use std::path::{Path, PathBuf};

use anyhow::Result;
use ignore::WalkBuilder;
use reposcry_graph::language::Language;
use tracing::{debug, info, warn};

use crate::preset::IndexPreset;

const DEFAULT_IGNORE: &[&str] = &[
    ".git/",
    "node_modules/",
    "bench/",
    "benchmarks/out/",
    "target/",
    "target-codex-test/",
    "target-*/",
    "dist/",
    "build/",
    ".next/",
    ".turbo/",
    "coverage/",
    ".cache/",
    ".reposcry/",
    ".code-review-graph/",
    "graphify-out/",
    "public/static/charting_library/",
    "*.min.js",
    "*.map",
    "*.lock",
    "*.pdb",
    "*.wasm",
    "*.png",
    "*.jpg",
    "*.jpeg",
    "*.webp",
    "*.gif",
    "*.svg",
    "*.ico",
    "*.mp4",
    "*.mp3",
    "*.wav",
    "*.ogg",
    "*.zip",
    "*.tar",
    "*.gz",
    "*.bz2",
    "*.7z",
    "*.rar",
    "*.pdf",
    "*.ttf",
    "*.otf",
    "*.woff",
    "*.woff2",
    "*.eot",
    "*.DS_Store",
    "Thumbs.db",
    ".gitkeep",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "Cargo.lock",
];

#[derive(Debug, Clone)]
pub struct ScannedFile {
    pub path: PathBuf,
    pub relative_path: String,
    pub language: String,
    pub size_bytes: u64,
}

pub struct FileScanner {
    repo_root: PathBuf,
    additional_ignore: Vec<String>,
    preset: Option<IndexPreset>,
}

impl FileScanner {
    pub fn new(repo_root: &Path) -> Self {
        let repo_root = repo_root.to_path_buf();
        let additional_ignore = read_reposcryignore(&repo_root);
        Self {
            repo_root,
            additional_ignore,
            preset: None,
        }
    }

    pub fn with_preset(mut self, preset: IndexPreset) -> Self {
        self.preset = Some(preset);
        self
    }

    pub fn add_ignore_pattern(mut self, pattern: &str) -> Self {
        self.additional_ignore.push(pattern.to_string());
        self
    }

    fn should_ignore(&self, relative_path: &str) -> bool {
        let relative_path = normalize_path(relative_path);
        for pattern in DEFAULT_IGNORE {
            if ignore_pattern_matches(&relative_path, pattern) {
                return true;
            }
        }
        for pattern in &self.additional_ignore {
            if ignore_pattern_matches(&relative_path, pattern) {
                return true;
            }
        }
        if let Some(ref preset) = self.preset {
            for pattern in &preset.additional_ignore {
                if ignore_pattern_matches(&relative_path, pattern) {
                    return true;
                }
            }
        }
        false
    }

    fn language_from_path(path: &Path) -> String {
        let path_str = path.to_string_lossy().replace('\\', "/");
        let language = Language::from_extension(&path_str);
        let language_id = language.as_str();
        if language_id == "unknown" {
            String::new()
        } else {
            language_id.to_string()
        }
    }

    pub fn scan(&self) -> Result<Vec<ScannedFile>> {
        let mut builder = WalkBuilder::new(&self.repo_root);
        builder.git_ignore(true);
        builder.git_global(true);
        builder.ignore(true);
        builder.follow_links(false);
        builder.max_depth(Some(50));

        let walker = builder.build();
        let mut files = Vec::new();
        for entry in walker {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let relative = path
                .strip_prefix(&self.repo_root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            if self.should_ignore(&relative) {
                debug!("Ignoring {}", relative);
                continue;
            }
            if relative.starts_with('.') && !relative.starts_with(".env") {
                continue;
            }
            let language = Self::language_from_path(path);
            if language.is_empty() {
                continue;
            }
            let metadata = std::fs::metadata(path)?;
            let size = metadata.len();
            if size > 1024 * 1024 {
                warn!(
                    "Skipping large file (>{:.1} MB): {}",
                    size as f64 / 1024.0 / 1024.0,
                    relative
                );
                continue;
            }
            files.push(ScannedFile {
                path: path.to_path_buf(),
                relative_path: relative,
                language,
                size_bytes: size,
            });
        }
        info!("Scanned {} files", files.len());
        Ok(files)
    }
}

fn read_reposcryignore(repo_root: &Path) -> Vec<String> {
    let path = repo_root.join(".reposcryignore");
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(ToOwned::to_owned)
        .collect()
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_string()
}

fn ignore_pattern_matches(relative_path: &str, pattern: &str) -> bool {
    let pattern = normalize_path(pattern.trim());
    if pattern.is_empty() || pattern.starts_with('#') {
        return false;
    }

    if let Some(prefix) = pattern.strip_suffix("*/") {
        return relative_path.starts_with(prefix);
    }

    if let Some(dir) = pattern.strip_suffix('/') {
        return relative_path == dir || relative_path.starts_with(&format!("{dir}/"));
    }

    if let Some(suffix) = pattern.strip_prefix("*.") {
        return relative_path.ends_with(&format!(".{suffix}"));
    }

    if let Some(suffix) = pattern.strip_prefix('*') {
        return relative_path.ends_with(suffix);
    }

    relative_path == pattern || relative_path.starts_with(&format!("{pattern}/"))
}

#[cfg(test)]
mod tests {
    use super::{ignore_pattern_matches, FileScanner};
    use std::path::Path;

    #[test]
    fn default_ignores_benchmark_fixture_repos() {
        let scanner = FileScanner::new(Path::new("."));
        assert!(scanner.should_ignore("bench/open-design/apps/daemon/src/chat-routes.ts"));
        assert!(scanner.should_ignore("benchmarks/out/latest.json"));
        assert!(scanner.should_ignore(".code-review-graph/cache/ast/file.json"));
        assert!(scanner.should_ignore("graphify-out/cache/ast/file.json"));
    }

    #[test]
    fn ignore_patterns_match_common_forms() {
        assert!(ignore_pattern_matches("bench/open-design/src/main.ts", "bench/"));
        assert!(ignore_pattern_matches("target-codex-test/debug/app.pdb", "target-*/"));
        assert!(ignore_pattern_matches("src/app.min.js", "*.min.js"));
        assert!(ignore_pattern_matches("Cargo.lock", "Cargo.lock"));
        assert!(!ignore_pattern_matches("src/benchmarks/mod.rs", "bench/"));
    }
}
