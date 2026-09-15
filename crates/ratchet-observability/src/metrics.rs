use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Per-task metrics and cost tracking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskMetrics {
    pub task_id: String,
    pub model: String,
    pub provider: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    pub estimated_cost_usd: f64,
    pub verification_passed: bool,
    pub human_interventions: u32,
    pub attempts: u32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct UsageAggregate {
    pub total_tasks: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cached_tokens: u64,
    pub total_estimated_cost_usd: f64,
    pub total_human_interventions: u64,
    pub tasks_passed: u64,
    pub tasks_failed: u64,
    #[serde(default)]
    pub by_provider: HashMap<String, ProviderAggregate>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProviderAggregate {
    pub tasks: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub estimated_cost_usd: f64,
}

impl UsageAggregate {
    /// Fold a slice of task metrics into an aggregate, ignoring anything
    /// started before `since`.
    pub fn from_metrics(metrics: &[TaskMetrics], since: DateTime<Utc>) -> Self {
        let mut agg = UsageAggregate::default();
        for m in metrics {
            if m.started_at < since {
                continue;
            }
            agg.total_tasks += 1;
            agg.total_input_tokens += m.input_tokens;
            agg.total_output_tokens += m.output_tokens;
            agg.total_cached_tokens += m.cached_tokens;
            agg.total_estimated_cost_usd += m.estimated_cost_usd;
            agg.total_human_interventions += m.human_interventions as u64;
            if m.verification_passed {
                agg.tasks_passed += 1;
            } else {
                agg.tasks_failed += 1;
            }

            let provider = agg.by_provider.entry(m.provider.clone()).or_default();
            provider.tasks += 1;
            provider.input_tokens += m.input_tokens;
            provider.output_tokens += m.output_tokens;
            provider.estimated_cost_usd += m.estimated_cost_usd;
        }
        agg
    }
}

pub struct CostTracker {
    metrics: Vec<TaskMetrics>,
}

impl CostTracker {
    pub fn new() -> Self {
        Self { metrics: Vec::new() }
    }

    pub fn record(&mut self, metric: TaskMetrics) {
        self.metrics.push(metric);
    }

    pub fn aggregate_since(&self, since: DateTime<Utc>) -> UsageAggregate {
        UsageAggregate::from_metrics(&self.metrics, since)
    }

    pub fn all_metrics(&self) -> &[TaskMetrics] {
        &self.metrics
    }
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new()
    }
}
