use anyhow::Result;
use chrono::{Duration, Utc};
use ratchet_observability::{metrics_path, MetricsStore, ReportFormat, Reporter};
use std::path::Path;

pub async fn run(project_dir: &Path, format: ReportFormat, since_days: u64) -> Result<()> {
    let since = Utc::now() - Duration::days(since_days as i64);
    let store = MetricsStore::new(metrics_path(&project_dir.join(".ratchet")));

    let aggregate = store.aggregate_since(since).await?;
    let reporter = Reporter::new();

    println!("{}", reporter.generate(&aggregate, format));
    Ok(())
}
