use anyhow::Result;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use ratchet_a2a::{
    A2aError, A2aResult, AgentCard, Task, TaskSendParams, TaskState, TaskStatus,
};
use ratchet_core::{AgentHarness, RunOverrides};
use ratchet_observability::{metrics_path, MetricsStore};
use ratchet_server::{
    a2a::{A2aDispatcher, A2aHandler},
    dashboard::DashboardSource,
    server::RatchetServer,
};
use ratchet_spec::SpecParser;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Exposes project metrics to the dashboard.
struct ProjectDashboard {
    ratchet_dir: PathBuf,
}

#[async_trait]
impl DashboardSource for ProjectDashboard {
    async fn summary(&self) -> Value {
        let store = MetricsStore::new(metrics_path(&self.ratchet_dir));
        match store.aggregate_since(Utc::now() - Duration::days(30)).await {
            Ok(agg) => serde_json::to_value(agg).unwrap_or_else(|_| json!({})),
            Err(_) => json!({"total_tasks": 0}),
        }
    }

    async fn tasks(&self) -> Value {
        let store = MetricsStore::new(metrics_path(&self.ratchet_dir));
        match store.load_all().await {
            Ok(mut records) => {
                // Most recent first.
                records.reverse();
                records.truncate(100);
                serde_json::to_value(records).unwrap_or_else(|_| json!([]))
            }
            Err(_) => json!([]),
        }
    }

    async fn specs(&self) -> Value {
        let dir = self.ratchet_dir.join("spec");
        let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
            return json!([]);
        };

        let parser = SpecParser::new();
        let mut specs = Vec::new();

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let Ok(content) = tokio::fs::read_to_string(&path).await else {
                continue;
            };
            if let Ok(spec) = parser.parse(&content) {
                specs.push(json!({
                    "id": spec.frontmatter.id,
                    "title": spec.frontmatter.title,
                    "status": format!("{:?}", spec.frontmatter.status).to_lowercase(),
                    "priority": format!("{:?}", spec.frontmatter.priority).to_lowercase(),
                }));
            }
        }

        specs.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        json!(specs)
    }
}

/// Exposes spec-driven runs to peer agents over A2A.
struct ProjectAgent {
    harness: Arc<Mutex<AgentHarness>>,
    spec_dir: PathBuf,
    tasks: Arc<Mutex<HashMap<String, Task>>>,
    url: String,
}

impl ProjectAgent {
    fn load_spec(&self, spec_id: &str) -> A2aResult<ratchet_spec::SpecFile> {
        let path = self.spec_dir.join(format!("{spec_id}.spec.md"));
        if !path.exists() {
            return Err(A2aError::InvalidParams(format!(
                "spec '{spec_id}' not found"
            )));
        }
        SpecParser::new()
            .parse_file(&path)
            .map_err(|e| A2aError::Internal(e.to_string()))
    }
}

#[async_trait]
impl A2aHandler for ProjectAgent {
    fn agent_card(&self) -> AgentCard {
        AgentCard::ratchet(format!("{}/a2a", self.url), env!("CARGO_PKG_VERSION"))
    }

    async fn send(&self, params: TaskSendParams) -> A2aResult<Task> {
        let spec_id = params.spec_id().ok_or_else(|| {
            A2aError::InvalidParams(
                "no spec referenced; set metadata.spec_id or include `spec:<id>`".to_string(),
            )
        })?;

        let spec = self.load_spec(&spec_id)?;

        let id = params
            .id
            .clone()
            .unwrap_or_else(|| format!("{spec_id}-{}", short_stamp()));

        // Idempotent: a repeated id returns the existing task.
        if let Some(existing) = self.tasks.lock().await.get(&id) {
            return Ok(existing.clone());
        }

        let mut task = Task::new(id.clone());
        task.session_id = params.session_id.clone();
        task.transition(TaskStatus::with_message(
            TaskState::Submitted,
            format!("accepted: {spec_id}"),
        ));

        self.tasks.lock().await.insert(id.clone(), task.clone());
        self.spawn_run(id, spec);
        Ok(task)
    }

    async fn get(&self, id: &str) -> A2aResult<Task> {
        self.tasks
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| A2aError::TaskNotFound(id.to_string()))
    }

    async fn cancel(&self, id: &str) -> A2aResult<Task> {
        let mut guard = self.tasks.lock().await;
        let task = guard
            .get_mut(id)
            .ok_or_else(|| A2aError::TaskNotFound(id.to_string()))?;

        if task.status.terminal() {
            return Err(A2aError::NotCancelable(format!(
                "{:?}",
                task.status.state
            )));
        }

        task.transition(TaskStatus::with_message(TaskState::Canceled, "canceled by peer"));
        Ok(task.clone())
    }
}

impl ProjectAgent {
    /// Run the spec in the background, updating the task as it progresses.
    fn spawn_run(&self, id: String, spec: ratchet_spec::SpecFile) {
        let harness = Arc::clone(&self.harness);
        let tasks = Arc::clone(&self.tasks);

        tokio::spawn(async move {
            let set = |state: TaskState, msg: String| {
                let tasks = Arc::clone(&tasks);
                let id = id.clone();
                async move {
                    if let Some(task) = tasks.lock().await.get_mut(&id) {
                        task.transition(TaskStatus::with_message(state, msg));
                    }
                }
            };

            set(TaskState::Working, "planning".to_string()).await;

            let mut guard = harness.lock().await;

            let plan = match guard.load_or_plan(&spec).await {
                Ok(plan) => plan,
                Err(e) => {
                    drop(guard);
                    set(TaskState::Failed, format!("planning failed: {e}")).await;
                    return;
                }
            };

            set(
                TaskState::Working,
                format!("executing {} task(s)", plan.task_graph.nodes.len()),
            )
            .await;

            match guard.run_with(&plan, &spec, RunOverrides::default()).await {
                Ok(report) => {
                    drop(guard);
                    let summary = report.verification.summary.clone();
                    let state = if report.verification.overall_passed {
                        TaskState::Completed
                    } else {
                        TaskState::Failed
                    };

                    let mut guard = tasks.lock().await;
                    if let Some(task) = guard.get_mut(&id) {
                        task.transition(TaskStatus::with_message(state, summary.clone()));
                        task.add_text_artifact(
                            "summary",
                            format!(
                                "{summary}\n\nCost: ${:.4}\nTasks: {}",
                                report.review.total_cost_usd, report.review.executed_tasks
                            ),
                        );
                        task.add_text_artifact("review", report.review.render());
                    }
                }
                Err(e) => {
                    drop(guard);
                    set(TaskState::Failed, e.to_string()).await;
                }
            }
        });
    }
}

fn short_stamp() -> String {
    Utc::now().format("%Y%m%d%H%M%S").to_string()
}

pub async fn run(
    project_dir: &Path,
    port: u16,
    a2a_enabled: bool,
    read_only: bool,
) -> Result<()> {
    let ratchet_dir = project_dir.join(".ratchet");

    let dashboard: Arc<dyn DashboardSource> = Arc::new(ProjectDashboard {
        ratchet_dir: ratchet_dir.clone(),
    });

    let a2a = if a2a_enabled {
        if read_only {
            anyhow::bail!("--read-only and --a2a are mutually exclusive");
        }
        let harness = super::helpers::build_harness(project_dir).await?;
        let url = format!("http://127.0.0.1:{port}");
        let agent = ProjectAgent {
            harness: Arc::new(Mutex::new(harness)),
            spec_dir: ratchet_dir.join("spec"),
            tasks: Arc::new(Mutex::new(HashMap::new())),
            url,
        };
        Arc::new(A2aDispatcher::new(Arc::new(agent)))
    } else {
        let closed: Arc<dyn A2aHandler> = Arc::new(ratchet_server::a2a::ClosedAgent {
            card: AgentCard::ratchet(format!("http://127.0.0.1:{port}/a2a"), env!("CARGO_PKG_VERSION")),
        });
        Arc::new(A2aDispatcher::new(closed))
    };

    println!(
        "🚀 Ratchet server on port {port}{}",
        if a2a_enabled {
            " (A2A accepting tasks)"
        } else {
            " (A2A read-only: pass --a2a to accept tasks)"
        }
    );

    RatchetServer::new(port, dashboard, a2a).run().await?;
    Ok(())
}
