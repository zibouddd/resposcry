use serde::{Deserialize, Serialize}
;
#[derive(Debug, Clone, Serialize, Deserialize)]pub struct FileMetrics {
pub file_path: String,
pub fan_in: u32,
pub fan_out: u32,
pub churn_score: f64,
pub complexity_score: f64,
pub risk_score: f64,
pub loc: u32,
pub num_symbols: u32,
pub num_tests: u32,}
impl FileMetrics {
pub fn compute_risk(&mut self) {
self.risk_score = self.fan_in as f64 * 0.30            + self.fan_out as f64 * 0.15            + self.churn_score * 0.25            + self.complexity_score * 0.20            + (if self.num_symbols > 10 { 1.0 }} else { 0.0 }
) * 0.10;    }
}

