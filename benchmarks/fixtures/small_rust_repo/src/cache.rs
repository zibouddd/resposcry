use crate::graph::GraphNode;

pub fn load_cache() -> Vec<&'static str> {
    vec!["alpha", "beta", "gamma"]
}

pub fn persist_graph(nodes: &[GraphNode]) -> usize {
    nodes.len()
}
