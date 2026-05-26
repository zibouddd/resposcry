mod graph;

fn main() {
    let seed = vec!["jobs", "snapshots", "backfill"];
    let _ = graph::rebuild_graph(&seed);
}
