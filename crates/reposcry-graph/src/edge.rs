use serde::{Deserialize, Serialize}
;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]pub enum EdgeKind {    Contains,    Imports,    Exports,    Calls,    References,    Implements,    Extends,    UsesType,    ReadsConfig,    WritesDatabase,    ReadsDatabase,    EmitsEvent,    ConsumesEvent,    Tests,    DependsOn,    ChangedWith,    OwnedBy,}
impl EdgeKind {
pub fn as_str(&self) -> &'static str {
match self {            EdgeKind::Contains => "contains",            EdgeKind::Imports => "imports",            EdgeKind::Exports => "exports",            EdgeKind::Calls => "calls",            EdgeKind::References => "references",            EdgeKind::Implements => "implements",            EdgeKind::Extends => "extends",            EdgeKind::UsesType => "uses_type",            EdgeKind::ReadsConfig => "reads_config",            EdgeKind::WritesDatabase => "writes_database",            EdgeKind::ReadsDatabase => "reads_database",            EdgeKind::EmitsEvent => "emits_event",            EdgeKind::ConsumesEvent => "consumes_event",            EdgeKind::Tests => "tests",            EdgeKind::DependsOn => "depends_on",            EdgeKind::ChangedWith => "changed_with",            EdgeKind::OwnedBy => "owned_by",        }
}
}
#[derive(Debug, Clone, Serialize, Deserialize)]pub struct GraphEdge {
pub source_id: u64,
pub target_id: u64,
pub kind: EdgeKind,
pub weight: f64,
pub metadata: Option<String>,}
impl GraphEdge {
pub fn new(source_id: u64, target_id: u64, kind: EdgeKind) -> Self {        Self {            source_id,            target_id,            kind,            weight: 1.0,            metadata: None,        }
}
}

