#[derive(Debug, Clone, PartialEq)]
pub struct CostModel {
    pub usd_per_million_input_tokens: f64,
    pub usd_per_million_output_tokens: f64,
    pub usd_per_million_cached_tokens: Option<f64>,
}

impl CostModel {
    pub fn estimate_cost(&self, input_tokens: u64, output_tokens: u64, cached_tokens: u64) -> f64 {
        let input_cost = (input_tokens as f64 / 1_000_000.0) * self.usd_per_million_input_tokens;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * self.usd_per_million_output_tokens;
        let cached_cost = self
            .usd_per_million_cached_tokens
            .map(|rate| (cached_tokens as f64 / 1_000_000.0) * rate)
            .unwrap_or(0.0);
        input_cost + output_cost + cached_cost
    }
}

impl Default for CostModel {
    fn default() -> Self {
        Self {
            usd_per_million_input_tokens: 0.0,
            usd_per_million_output_tokens: 0.0,
            usd_per_million_cached_tokens: None,
        }
    }
}
