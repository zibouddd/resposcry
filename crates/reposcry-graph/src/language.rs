use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Json,
    Toml,
    Yaml,
    Markdown,
    Css,
    Html,
    Sql,
    Go,
    Java,
    CSharp,
    Cpp,
    C,
    Kotlin,
    Swift,
    Php,
    Ruby,
    Lua,
    Dart,
    Scala,
    Svelte,
    Vue,
    Nix,
    Powershell,
    Unknown(String),
}

impl Language {
    pub fn from_extension(path: &str) -> Self {
        let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
        match ext.as_str() {
            "rs" => Language::Rust,
            "ts" | "tsx" => Language::TypeScript,
            "js" | "jsx" | "mjs" | "cjs" => Language::JavaScript,
            "py" | "pyw" => Language::Python,
            "json" => Language::Json,
            "toml" => Language::Toml,
            "yaml" | "yml" => Language::Yaml,
            "md" | "mdx" => Language::Markdown,
            "css" | "scss" | "sass" | "less" => Language::Css,
            "html" | "htm" => Language::Html,
            "sql" => Language::Sql,
            "go" => Language::Go,
            "java" => Language::Java,
            "cs" => Language::CSharp,
            "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => Language::Cpp,
            "c" | "h" => Language::C,
            "kt" | "kts" => Language::Kotlin,
            "swift" => Language::Swift,
            "php" => Language::Php,
            "rb" => Language::Ruby,
            "lua" => Language::Lua,
            "dart" => Language::Dart,
            "scala" | "sc" => Language::Scala,
            "svelte" => Language::Svelte,
            "vue" => Language::Vue,
            "nix" => Language::Nix,
            "ps1" | "psm1" | "psd1" => Language::Powershell,
            _ => Language::Unknown(ext),
        }
    }

    pub fn has_tree_sitter_parser(&self) -> bool {
        matches!(
            self,
            Language::Rust
                | Language::TypeScript
                | Language::JavaScript
                | Language::Python
                | Language::Json
                | Language::Toml
                | Language::Yaml
        )
    }

    pub fn support_level(&self) -> &'static str {
        if self.has_tree_sitter_parser() {
            "symbols-imports-calls"
        } else {
            "file-loc-language"
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::TypeScript => "typescript",
            Language::JavaScript => "javascript",
            Language::Python => "python",
            Language::Json => "json",
            Language::Toml => "toml",
            Language::Yaml => "yaml",
            Language::Markdown => "markdown",
            Language::Css => "css",
            Language::Html => "html",
            Language::Sql => "sql",
            Language::Go => "go",
            Language::Java => "java",
            Language::CSharp => "csharp",
            Language::Cpp => "cpp",
            Language::C => "c",
            Language::Kotlin => "kotlin",
            Language::Swift => "swift",
            Language::Php => "php",
            Language::Ruby => "ruby",
            Language::Lua => "lua",
            Language::Dart => "dart",
            Language::Scala => "scala",
            Language::Svelte => "svelte",
            Language::Vue => "vue",
            Language::Nix => "nix",
            Language::Powershell => "powershell",
            Language::Unknown(_) => "unknown",
        }
    }
}
