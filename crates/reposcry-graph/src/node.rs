use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum NodeKind {
    Repository,
    Package,
    Directory,
    File,
    Module,
    Symbol,
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Trait,
    Interface,
    Component,
    Test,
    Config,
    Route,
    DatabaseTable,
    ApiEndpoint,
    Hook,
    ServerAction,
}

impl NodeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeKind::Repository => "repository",
            NodeKind::Package => "package",
            NodeKind::Directory => "directory",
            NodeKind::File => "file",
            NodeKind::Module => "module",
            NodeKind::Symbol => "symbol",
            NodeKind::Function => "function",
            NodeKind::Method => "method",
            NodeKind::Class => "class",
            NodeKind::Struct => "struct",
            NodeKind::Enum => "enum",
            NodeKind::Trait => "trait",
            NodeKind::Interface => "interface",
            NodeKind::Component => "component",
            NodeKind::Test => "test",
            NodeKind::Config => "config",
            NodeKind::Route => "route",
            NodeKind::DatabaseTable => "database_table",
            NodeKind::ApiEndpoint => "api_endpoint",
            NodeKind::Hook => "hook",
            NodeKind::ServerAction => "server_action",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: u64,
    pub name: String,
    pub kind: NodeKind,
    pub file_path: Option<String>,
    pub language: Option<String>,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub signature: Option<String>,
    pub visibility: Option<String>,
    pub doc_comment: Option<String>,
}

impl GraphNode {
    pub fn new(id: u64, name: String, kind: NodeKind) -> Self {
        Self {
            id,
            name,
            kind,
            file_path: None,
            language: None,
            start_line: None,
            end_line: None,
            signature: None,
            visibility: None,
            doc_comment: None,
        }
    }
}
