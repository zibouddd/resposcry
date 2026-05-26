mod graph;

fn main() {
    let seed = vec!["jobs", "events", "cache"];
    let _ = graph::rebuild_graph(&seed);
}
