use serde::{Deserialize, Serialize}
;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]pub enum Language {    Rust,    TypeScript,    JavaScript,    Python,    Json,    Toml,    Yaml,    Unknown(String),}
impl Language {
pub fn from_extension(path: &str) -> Self {
let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();        match ext.as_str() {            "rs" => Language::Rust,            "ts" | "tsx" => Language::TypeScript,            "js" | "jsx" => Language::JavaScript,            "py" => Language::Python,            "json" => Language::Json,            "toml" => Language::Toml,            "yaml" | "yml" => Language::Yaml,            _ => Language::Unknown(ext),        }
}
pub fn has_tree_sitter_parser(&self) -> bool {        matches!(            self,            Language::Rust                | Language::TypeScript                | Language::JavaScript                | Language::Python                | Language::Json                | Language::Toml                | Language::Yaml        )    }
pub fn as_str(&self) -> &'static str {
match self {            Language::Rust => "rust",            Language::TypeScript => "typescript",            Language::JavaScript => "javascript",            Language::Python => "python",            Language::Json => "json",            Language::Toml => "toml",            Language::Yaml => "yaml",            Language::Unknown(_) => "unknown",        }
}
}

