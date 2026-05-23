use std::collections::{HashMap, VecDeque}
;
use petgraph::algo;
use petgraph::graph::{DiGraph, NodeIndex}
;
use petgraph::visit::EdgeRef;
use serde::{Deserialize, Serialize}
;
use crate::edge::{EdgeKind, GraphEdge}
;
use crate::node::{GraphNode, NodeKind}
;
#[derive(Debug, Clone, Serialize, Deserialize)]pub struct CodeGraph {
pub nodes: HashMap<u64, GraphNode>,
pub edges: Vec<GraphEdge>,
pub next_id: u64,}
pub struct PetGraphWrapper {
pub graph: DiGraph<GraphNode, EdgeKind>,
pub node_id_to_index: HashMap<u64, NodeIndex>,
pub index_to_node_id: HashMap<NodeIndex, u64>,}
impl CodeGraph {
pub fn new() -> Self {        Self {            nodes: HashMap::new(),            edges: Vec::new(),            next_id: 1,        }
}
pub fn add_node(&mut self, kind: NodeKind, name: &str) -> u64 {
let id = self.next_id;        self.next_id += 1;
let node = GraphNode::new(id, name.to_string(), kind);        self.nodes.insert(id, node);        id    }
pub fn add_edge(&mut self, source_id: u64, target_id: u64, kind: EdgeKind) {
let edge = GraphEdge::new(source_id, target_id, kind);        self.edges.push(edge);    }
pub fn get_node(&self, id: u64) -> Option<&GraphNode> {
self.nodes.get(&id)    }
pub fn find_nodes_by_name(&self, name: &str) -> Vec<&GraphNode> {
self.nodes            .values()            .filter(|n| n.name.contains(name))            .collect()    }
pub fn find_nodes_by_path(&self, path: &str) -> Vec<&GraphNode> {
self.nodes            .values()            .filter(|n| n.file_path.as_deref().map_or(false, |p| p == path))            .collect()    }
pub fn reverse_dependencies(&self, node_id: u64) -> Vec<&GraphEdge> {
self.edges            .iter()            .filter(|e| e.target_id == node_id)            .collect()    }
pub fn dependencies(&self, node_id: u64) -> Vec<&GraphEdge> {
self.edges            .iter()            .filter(|e| e.source_id == node_id)            .collect()    }
pub fn find_path(&self, from_id: u64, to_id: u64) -> Option<Vec<u64>> {
let pet = self.to_petgraph();
let from = *pet.node_id_to_index.get(&from_id)?;
let to = *pet.node_id_to_index.get(&to_id)?;
let path = algo::dijkstra(&pet.graph, from, Some(to), |_| 1);        if path.contains_key(&to) {
let result = vec![to_id];            Some(result)        }} else {            None        }
}
pub fn detect_cycles(&self) -> Vec<Vec<u64>> {
let pet = self.to_petgraph();
let mut cycles = Vec::new();        if algo::is_cyclic_directed(&pet.graph) {
let mut visited = HashMap::new();
let mut stack = Vec::new();            for node in pet.graph.node_indices() {
if visited.contains_key(&node) {                    continue;                }
if let Some(cycle) = self.dfs_find_cycle(node, &pet, &mut visited, &mut stack) {                    cycles.push(cycle);                }
}
}
cycles    }
fn dfs_find_cycle(        &self,        start: NodeIndex,        pet: &PetGraphWrapper,        visited: &mut HashMap<NodeIndex, bool>,        stack: &mut Vec<u64>,    ) -> Option<Vec<u64>> {        visited.insert(start, true);        stack.push(pet.index_to_node_id.get(&start).copied().unwrap_or(0));        for edge in pet.graph.edges(start) {
let target = edge.target();            if !visited.contains_key(&target) {
if let Some(cycle) = self.dfs_find_cycle(target, pet, visited, stack) {
return Some(cycle);                }
}} else if stack.contains(&pet.index_to_node_id.get(&target).copied().unwrap_or(0)) {
let cycle_start = stack                    .iter()                    .position(|&x| x == pet.index_to_node_id.get(&target).copied().unwrap_or(0))                    .unwrap_or(0);                return Some(stack[cycle_start..].to_vec());            }
}
stack.pop();        None    }
pub fn to_petgraph(&self) -> PetGraphWrapper {
let mut graph = DiGraph::new();
let mut node_id_to_index = HashMap::new();
let mut index_to_node_id = HashMap::new();        for (id, node) in &self.nodes {
let idx = graph.add_node(node.clone());            node_id_to_index.insert(*id, idx);            index_to_node_id.insert(idx, *id);        }
for edge in &self.edges {
if let (Some(&source_idx), Some(&target_idx)) =                (node_id_to_index.get(&edge.source_id), node_id_to_index.get(&edge.target_id)) {                graph.add_edge(source_idx, target_idx, edge.kind.clone());            }
}
PetGraphWrapper {            graph,            node_id_to_index,            index_to_node_id,        }
}
pub fn importance_scores(&self) -> HashMap<u64, f64> {
let pet = self.to_petgraph();
let mut scores: HashMap<u64, f64> = HashMap::new();
let num_nodes = pet.graph.node_count();        if num_nodes == 0 {
return scores;        }
let damping = 0.85;
let initial = 1.0 / num_nodes as f64;        for &id in self.nodes.keys() {            scores.insert(id, initial);        }
for _ in 0..20 {
let mut new_scores: HashMap<u64, f64> = HashMap::new();
let dangling_sum: f64 = scores.values().sum();            for (&id, _) in &self.nodes {
let mut score = (1.0 - damping) / num_nodes as f64;
let mut incoming_weight = 0.0;                for edge in &self.edges {
if edge.target_id == id {
let source_score = scores.get(&edge.source_id).copied().unwrap_or(initial);
let out_degree = self                            .edges                            .iter()                            .filter(|e| e.source_id == edge.source_id)                            .count();                        if out_degree > 0 {                            incoming_weight += source_score * edge.weight / out_degree as f64;                        }
}
}
score += damping * incoming_weight;                score += damping * dangling_sum / num_nodes as f64;                new_scores.insert(id, score);            }
scores = new_scores;        }
scores    }
pub fn bfs_reachable(&self, start_id: u64, max_depth: usize) -> Vec<u64> {
let mut visited = HashMap::new();
let mut queue = VecDeque::new();
let mut result = Vec::new();        queue.push_back((start_id, 0));        visited.insert(start_id, true);        while let Some((node_id, depth)) = queue.pop_front() {
if depth > max_depth {                continue;            }
result.push(node_id);            for edge in &self.edges {
if edge.source_id == node_id && !visited.contains_key(&edge.target_id) {                    visited.insert(edge.target_id, true);                    queue.push_back((edge.target_id, depth + 1));                }
}
}
result    }
}
impl Default for CodeGraph {
fn default() -> Self {        Self::new()    }
}

