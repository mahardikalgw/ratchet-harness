use crate::metrics::UsageAggregate;

/// Generates human-readable reports from metrics.
pub struct Reporter;

impl Reporter {
    pub fn new() -> Self {
        Self
    }

    pub fn generate(&self, aggregate: &UsageAggregate, format: ReportFormat) -> String {
        match format {
            ReportFormat::Text => self.text_report(aggregate),
            ReportFormat::Json => serde_json::to_string_pretty(aggregate).unwrap_or_default(),
            ReportFormat::Markdown => self.markdown_report(aggregate),
        }
    }

    fn text_report(&self, agg: &UsageAggregate) -> String {
        format!(
            "Ratchet Usage Report\n====================\n\
            Total tasks: {}\n\
            Passed: {} | Failed: {}\n\
            Total tokens: {} in / {} out / {} cached\n\
            Estimated cost: ${:.4}\n\
            Human interventions: {}\n\n\
            By Provider:\n\
            {}",
            agg.total_tasks,
            agg.tasks_passed,
            agg.tasks_failed,
            agg.total_input_tokens,
            agg.total_output_tokens,
            agg.total_cached_tokens,
            agg.total_estimated_cost_usd,
            agg.total_human_interventions,
            agg.by_provider
                .iter()
                .map(|(name, p)| format!(
                    "  {}: {} tasks, {} tokens, ${:.4}",
                    name,
                    p.tasks,
                    p.input_tokens + p.output_tokens,
                    p.estimated_cost_usd
                ))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }

    fn markdown_report(&self, agg: &UsageAggregate) -> String {
        format!(
            "# Ratchet Usage Report\n\n\
            | Metric | Value |\n\
            |--------|-------|\n\
            | Total Tasks | {} |\n\
            | Passed | {} |\n\
            | Failed | {} |\n\
            | Input Tokens | {} |\n\
            | Output Tokens | {} |\n\
            | Cached Tokens | {} |\n\
            | Est. Cost | ${:.4} |\n\
            | Human Interventions | {} |\n\n\
            ## By Provider\n\n\
            | Provider | Tasks | Tokens | Cost |\n\
            |----------|-------|--------|------|\n\
            {}",
            agg.total_tasks,
            agg.tasks_passed,
            agg.tasks_failed,
            agg.total_input_tokens,
            agg.total_output_tokens,
            agg.total_cached_tokens,
            agg.total_estimated_cost_usd,
            agg.total_human_interventions,
            agg.by_provider
                .iter()
                .map(|(name, p)| format!(
                    "| {} | {} | {} | ${:.4} |",
                    name,
                    p.tasks,
                    p.input_tokens + p.output_tokens,
                    p.estimated_cost_usd
                ))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

impl Default for Reporter {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReportFormat {
    Text,
    Json,
    Markdown,
}
