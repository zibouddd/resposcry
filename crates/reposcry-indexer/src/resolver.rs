use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

pub trait ImportResolver {
    fn resolve(&self, from_file: &Path, raw_import: &str) -> Option<PathBuf>;
}

// ── TypeScript Import Resolver ───────────────────────────

pub struct TypeScriptImportResolver {
    repo_root: PathBuf,
    aliases: HashMap<String, PathBuf>,
}

impl TypeScriptImportResolver {
    pub fn new(repo_root: &Path) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
            aliases: HashMap::new(),
        }
    }

    pub fn with_alias(mut self, alias: &str, path: &str) -> Self {
        self.aliases.insert(alias.to_string(), PathBuf::from(path));
        self
    }

    pub fn load_tsconfig(&mut self, tsconfig_path: &Path) -> Result<()> {
        if !tsconfig_path.exists() {
            return Ok(());
        }
        let content = std::fs::read_to_string(tsconfig_path)?;
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(compiler_opts) = json.get("compilerOptions") {
                if let Some(paths) = compiler_opts.get("paths").and_then(|p| p.as_object()) {
                    for (key, val) in paths {
                        let alias_key = key.trim_end_matches("/*");
                        if let Some(first) = val.as_array().and_then(|a| a.first()) {
                            if let Some(path_str) = first.as_str() {
                                let resolved =
                                    PathBuf::from(path_str.trim_end_matches("/*"));
                                self.aliases.insert(alias_key.to_string(), resolved);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

impl ImportResolver for TypeScriptImportResolver {
    fn resolve(&self, from_file: &Path, raw_import: &str) -> Option<PathBuf> {
        if raw_import.starts_with('.') {
            resolve_relative(from_file, raw_import)
        } else if raw_import.starts_with('@') {
            let parts: Vec<&str> = raw_import.split('/').collect();
            let alias = parts[0];
            let rest = parts[1..].join("/");
            if let Some(alias_path) = self.aliases.get(alias) {
                let full_path = self.repo_root.join(alias_path).join(&rest);
                resolve_with_extensions(&full_path)
            } else {
                None
            }
        } else if raw_import.starts_with("~") {
            let rest = &raw_import[1..];
            let full_path = self.repo_root.join(&rest);
            resolve_with_extensions(&full_path)
        } else {
            None
        }
    }
}

// ── Rust Import Resolver ─────────────────────────────────

pub struct RustImportResolver {
    _repo_root: PathBuf,
    crate_root: PathBuf,
}

impl RustImportResolver {
    pub fn new(repo_root: &Path) -> Self {
        let crate_root = find_crate_root(repo_root)
            .unwrap_or_else(|| repo_root.to_path_buf());
        Self {
            _repo_root: repo_root.to_path_buf(),
            crate_root,
        }
    }
}

fn find_crate_root(path: &Path) -> Option<PathBuf> {
    let mut current = Some(path.to_path_buf());
    while let Some(dir) = current {
        if dir.join("Cargo.toml").exists() {
            return Some(dir);
        }
        if dir.join("src").join("lib.rs").exists()
            || dir.join("src").join("main.rs").exists()
        {
            return Some(dir);
        }
        current = dir.parent().map(|p| p.to_path_buf());
    }
    None
}

impl ImportResolver for RustImportResolver {
    fn resolve(&self, from_file: &Path, raw_import: &str) -> Option<PathBuf> {
        let clean: Vec<&str> = raw_import.split("::").collect();
        if raw_import.starts_with("crate::") {
            let path_parts: Vec<&str> = clean[1..].to_vec();
            let relative = path_parts.join("/");
            let full = self.crate_root.join("src").join(&relative);
            resolve_with_extensions(&full)
        } else if raw_import.starts_with("super::") {
            let parent = from_file.parent()?;
            let rest = raw_import.strip_prefix("super::")?.replace("::", "/");
            let full = parent.parent()?.join(&rest);
            resolve_with_extensions(&full)
        } else if raw_import.starts_with("self::") {
            let parent = from_file.parent()?;
            let rest = raw_import.strip_prefix("self::")?.replace("::", "/");
            let full = parent.join(&rest);
            resolve_with_extensions(&full)
        } else if !raw_import.contains("::") {
            None
        } else {
            None
        }
    }
}

// ── Helpers ──────────────────────────────────────────────

const EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "rs", "py", "json"];

fn resolve_relative(from_file: &Path, raw_import: &str) -> Option<PathBuf> {
    let parent = from_file.parent()?;
    let relative = raw_import.trim_start_matches("./");
    let full = parent.join(relative);
    resolve_with_extensions(&full)
}

fn resolve_with_extensions(path: &Path) -> Option<PathBuf> {
    if path.exists() && path.is_file() {
        return Some(path.to_path_buf());
    }
    for ext in EXTENSIONS {
        let with_ext = path.with_extension(ext);
        if with_ext.exists() {
            return Some(with_ext);
        }
    }
    for ext in EXTENSIONS {
        let index = path.join(format!("index.{}", ext));
        if index.exists() {
            return Some(index);
        }
    }
    None
}
