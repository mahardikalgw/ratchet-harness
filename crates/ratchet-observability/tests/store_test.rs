use chrono::{Duration, Utc};
use ratchet_observability::{MetricsStore, TaskMetrics, metrics_path};

fn metric(task_id: &str, provider: &str, cost: f64, passed: bool) -> TaskMetrics {
    TaskMetrics {
        task_id: task_id.to_string(),
        model: "test-model".to_string(),
        provider: provider.to_string(),
        started_at: Utc::now(),
        finished_at: Some(Utc::now()),
        input_tokens: 100,
        output_tokens: 50,
        cached_tokens: 0,
        estimated_cost_usd: cost,
        verification_passed: passed,
        human_interventions: 0,
        attempts: 1,
    }
}

#[tokio::test]
async fn append_and_load_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let store = MetricsStore::new(metrics_path(dir.path()));

    store
        .append(&metric("T-1", "deepseek", 0.01, true))
        .await
        .unwrap();
    store
        .append(&metric("T-2", "claude", 0.50, false))
        .await
        .unwrap();

    let loaded = store.load_all().await.unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].task_id, "T-1");
    assert_eq!(loaded[1].provider, "claude");
}

#[tokio::test]
async fn load_missing_file_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let store = MetricsStore::new(metrics_path(dir.path()));
    assert!(store.load_all().await.unwrap().is_empty());
}

#[tokio::test]
async fn aggregation_sums_tokens_and_cost() {
    let dir = tempfile::tempdir().unwrap();
    let store = MetricsStore::new(metrics_path(dir.path()));

    store
        .append(&metric("T-1", "deepseek", 0.01, true))
        .await
        .unwrap();
    store
        .append(&metric("T-2", "deepseek", 0.02, true))
        .await
        .unwrap();
    store
        .append(&metric("T-3", "claude", 0.50, false))
        .await
        .unwrap();

    let agg = store
        .aggregate_since(Utc::now() - Duration::hours(1))
        .await
        .unwrap();
    assert_eq!(agg.total_tasks, 3);
    assert_eq!(agg.total_input_tokens, 300);
    assert_eq!(agg.total_output_tokens, 150);
    assert_eq!(agg.tasks_passed, 2);
    assert_eq!(agg.tasks_failed, 1);
    assert!((agg.total_estimated_cost_usd - 0.53).abs() < 1e-9);

    let deepseek = &agg.by_provider["deepseek"];
    assert_eq!(deepseek.tasks, 2);
    assert!((deepseek.estimated_cost_usd - 0.03).abs() < 1e-9);
}

#[tokio::test]
async fn aggregation_honors_since_cutoff() {
    let dir = tempfile::tempdir().unwrap();
    let store = MetricsStore::new(metrics_path(dir.path()));

    store
        .append(&metric("T-1", "deepseek", 0.01, true))
        .await
        .unwrap();

    // Cutoff in the future excludes all existing records.
    let agg = store
        .aggregate_since(Utc::now() + Duration::hours(1))
        .await
        .unwrap();
    assert_eq!(agg.total_tasks, 0);
}

#[tokio::test]
async fn malformed_lines_are_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let path = metrics_path(dir.path());
    let store = MetricsStore::new(path.clone());

    store
        .append(&metric("T-1", "deepseek", 0.01, true))
        .await
        .unwrap();

    // Append a corrupt line by hand.
    let mut content = tokio::fs::read_to_string(&path).await.unwrap();
    content.push_str("this is not json\n");
    tokio::fs::write(&path, content).await.unwrap();

    let loaded = store.load_all().await.unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].task_id, "T-1");
}
