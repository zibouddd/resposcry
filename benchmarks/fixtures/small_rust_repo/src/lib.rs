pub mod cache;
pub mod graph;

pub fn run() {
    let seed = cache::load_cache();
    let graph = graph::rebuild_graph(&seed);
    cache::persist_graph(&graph);
}
