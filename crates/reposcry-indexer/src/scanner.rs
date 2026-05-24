use std::path::{Path, PathBuf};

use anyhow::Result;
use ignore::WalkBuilder;
use reposcry_graph::language::Language;
use tracing::{debug, info, warn};

use crate::preset::IndexPreset;

const DEFAULT_IGNORE: &[&str] = &[
    ".git/",
    "node_modules/",
    "target/",
    "target-codex-test/",
    "dist/",
    "build/",
    ".next/",
    ".turbo/",
    "coverage/",
    ".cache/",
    ".reposcry/",
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
        Self {
            repo_root: repo_root.to_path_buf(),
            additional_ignore: Vec::new(),
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
        for pattern in DEFAULT_IGNORE {
            if relative_path.starts_with(pattern.trim_end_matches('/'))
                || relative_path.ends_with(pattern.trim_start_matches('*'))
                || relative_path.contains(pattern.trim_matches('*'))
            {
                return true;
            }
        }
        for pattern in &self.additional_ignore {
            if relative_path.starts_with(pattern.trim_end_matches('/')) {
                return true;
            }
        }
        if let Some(ref preset) = self.preset {
            for pattern in &preset.additional_ignore {
                if relative_path.starts_with(pattern.trim_end_matches('/')) {
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
        builder.ignore(false);
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
