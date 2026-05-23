use small_rust_repo::{cache, graph};

#[test]
fn rebuild_graph_creates_nodes() {
    let seed = cache::load_cache();
    let graph = graph::rebuild_graph(&seed);
    assert_eq!(graph.len(), 3);
}
