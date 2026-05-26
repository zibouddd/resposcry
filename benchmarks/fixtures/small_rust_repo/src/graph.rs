#[derive(Clone, Debug)]
pub struct GraphNode {
    pub name: String,
    pub edges: usize,
}

pub fn rebuild_graph(seed: &[&str]) -> Vec<GraphNode> {
    seed.iter()
        .enumerate()
        .map(|(index, value)| GraphNode {
            name: normalize_node(value),
            edges: index + 1,
        })
        .collect()
}

fn normalize_node(value: &str) -> String {
    format!("node::{value}")
}
