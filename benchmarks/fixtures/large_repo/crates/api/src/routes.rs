pub fn rebuild_graph(seed: &[&str]) -> Vec<String> {
    seed.iter().map(|value| format!("api::{value}")).collect()
}

pub fn rebuild_handler() -> usize {
    rebuild_graph(&["route", "cache", "service"]).len()
}
