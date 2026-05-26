pub fn rebuild_graph(seed: &[&str]) -> Vec<String> {
    seed.iter().map(|value| format!("indexer::{value}")).collect()
}
