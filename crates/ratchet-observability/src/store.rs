use crate::{
    error::ObservabilityResult,
    metrics::{TaskMetrics, UsageAggregate},
};
use chrono::{DateTime, Utc};
use std::path::PathBuf;

/// Append-only JSONL store for task metrics.
///
/// One JSON object per line, so partial writes never corrupt earlier records
/// and the file can be tailed or grepped directly.
pub struct MetricsStore {
    path: PathBuf,
}

impl MetricsStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Append a single metric record.
    pub async fn append(&self, metric: &TaskMetrics) -> ObservabilityResult<()> {
        use tokio::io::AsyncWriteExt;

        let line = serde_json::to_string(metric)?;
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;
        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;
        Ok(())
    }

    /// Load every record. Malformed lines are skipped rather than failing the
    /// whole read, so a truncated tail can't render reports unusable.
    pub async fn load_all(&self) -> ObservabilityResult<Vec<TaskMetrics>> {
        let content = match tokio::fs::read_to_string(&self.path).await {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };

        let mut out = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<TaskMetrics>(line) {
                Ok(m) => out.push(m),
                Err(e) => {
                    tracing::warn!(error = %e, "skipping malformed metrics line");
                }
            }
        }
        Ok(out)
    }

    /// Load records and aggregate everything since `since`.
    pub async fn aggregate_since(
        &self,
        since: DateTime<Utc>,
    ) -> ObservabilityResult<UsageAggregate> {
        let metrics = self.load_all().await?;
        Ok(UsageAggregate::from_metrics(&metrics, since))
    }
}

/// Convenience: build the default metrics path under a `.ratchet` dir.
pub fn metrics_path(ratchet_dir: &std::path::Path) -> PathBuf {
    ratchet_dir.join("metrics.jsonl")
}
