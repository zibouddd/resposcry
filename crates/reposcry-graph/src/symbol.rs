use serde::{Deserialize, Serialize}
;
#[derive(Debug, Clone, Serialize, Deserialize)]pub struct Symbol {
pub id: Option<i64>,
pub file_path: String,
pub name: String,
pub kind: String,
pub start_line: u32,
pub end_line: u32,
pub signature: Option<String>,
pub visibility: Option<String>,
pub doc_comment: Option<String>,}
#[derive(Debug, Clone, Serialize, Deserialize)]pub struct Import {
pub source: String,
pub target: String,
pub is_relative: bool,
pub imported_names: Vec<String>,
pub line: u32,}
#[derive(Debug, Clone, Serialize, Deserialize)]pub struct CallSite {
pub caller: String,
pub callee: String,
pub file: String,
pub line: u32,}
#[derive(Debug, Clone, Serialize, Deserialize)]pub struct TestCase {
pub name: String,
pub file: String,
pub line: u32,
pub is_async: bool,}
#[derive(Debug, Clone, Serialize, Deserialize)]pub struct ParsedFile {
pub path: String,
pub language: String,
pub symbols: Vec<Symbol>,
pub imports: Vec<Import>,
pub calls: Vec<CallSite>,
pub tests: Vec<TestCase>,
pub hash: String,
pub size_bytes: u64,
pub loc: u32,}

