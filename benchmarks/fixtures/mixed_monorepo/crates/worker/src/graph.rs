pub fn rebuild_graph(seed: &[&str]) -> Vec<String> {
    seed.iter().map(|value| format!("worker::{value}")).collect()
}
