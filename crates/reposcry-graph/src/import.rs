use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]pub enum ImportKind {    Relative,    Absolute,    Package,    Module,    Alias,}
#[derive(Debug, Clone, Serialize, Deserialize)]pub struct ResolvedImport {    pub raw: String,    pub resolved_path: Option<String>,    pub kind: ImportKind,    pub imported_names: Vec<String>,    pub is_external_package: bool,}
