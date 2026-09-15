use ratchet_a2a::{A2aError, A2aResult, AgentCard, Task, TaskSendParams, TaskState, TaskStatus};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Operations Ratchet exposes to peer agents.
#[async_trait::async_trait]
pub trait A2aHandler: Send + Sync {
    /// The card peers discover.
    fn agent_card(&self) -> AgentCard;

    /// Accept a delegated task and return it (possibly still running).
    async fn send(&self, params: TaskSendParams) -> A2aResult<Task>;

    /// Look up a task.
    async fn get(&self, id: &str) -> A2aResult<Task>;

    /// Request cancellation.
    async fn cancel(&self, id: &str) -> A2aResult<Task>;
}

/// Tracks delegated tasks and dispatches them to a handler.
///
/// `tasks/send` returns as soon as work is accepted; callers poll `tasks/get`
/// for the outcome rather than holding a connection open. That keeps the
/// server responsive while a spec-driven run takes minutes.
pub struct A2aDispatcher {
    handler: Arc<dyn A2aHandler>,
    tasks: Arc<Mutex<HashMap<String, Task>>>,
}

impl A2aDispatcher {
    pub fn new(handler: Arc<dyn A2aHandler>) -> Self {
        Self {
            handler,
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn card(&self) -> AgentCard {
        self.handler.agent_card()
    }

    /// Handle a JSON-RPC method call and return its result payload.
    pub async fn handle(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> A2aResult<serde_json::Value> {
        match method {
            "tasks/send" | "tasks/sendSubscribe" => {
                let params: TaskSendParams = serde_json::from_value(params)
                    .map_err(|e| A2aError::InvalidParams(e.to_string()))?;

                let task = self.handler.send(params).await?;
                let id = task.id.clone();
                let terminal = task.status.terminal();

                self.tasks.lock().await.insert(id.clone(), task.clone());

                if !terminal {
                    self.spawn_poller(id);
                }

                to_json(task)
            }

            "tasks/get" => {
                let id = extract_id(&params)?;
                let task = self.lookup(&id).await?;
                to_json(task)
            }

            "tasks/cancel" => {
                let id = extract_id(&params)?;

                if let Some(existing) = self.tasks.lock().await.get(&id) {
                    if existing.status.terminal() {
                        return Err(A2aError::NotCancelable(
                            format!("{:?}", existing.status.state).to_lowercase(),
                        ));
                    }
                }

                let mut task = self.handler.cancel(&id).await?;
                task.id = id.clone();
                self.tasks.lock().await.insert(id, task.clone());
                to_json(task)
            }

            other => Err(A2aError::UnsupportedOperation(other.to_string())),
        }
    }

    /// Poll the handler until the task reaches a terminal state, keeping the
    /// local cache current for `tasks/get`.
    fn spawn_poller(&self, id: String) {
        let handler = Arc::clone(&self.handler);
        let tasks = Arc::clone(&self.tasks);

        tokio::spawn(async move {
            // Bounded so a handler that never finishes cannot leak a task.
            for _ in 0..7_200 {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                {
                    let guard = tasks.lock().await;
                    match guard.get(&id) {
                        Some(task) if task.status.terminal() => return,
                        // Cancelled and evicted — stop polling.
                        None => return,
                        _ => {}
                    }
                }

                match handler.get(&id).await {
                    Ok(task) => {
                        let done = task.status.terminal();
                        tasks.lock().await.insert(id.clone(), task);
                        if done {
                            return;
                        }
                    }
                    Err(_) => return,
                }
            }
        });
    }

    async fn lookup(&self, id: &str) -> A2aResult<Task> {
        if let Some(task) = self.tasks.lock().await.get(id) {
            return Ok(task.clone());
        }
        let task = self.handler.get(id).await?;
        self.tasks.lock().await.insert(id.to_string(), task.clone());
        Ok(task)
    }
}

fn to_json(task: Task) -> A2aResult<serde_json::Value> {
    serde_json::to_value(task).map_err(|e| A2aError::Internal(e.to_string()))
}

fn extract_id(params: &serde_json::Value) -> A2aResult<String> {
    params
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| A2aError::InvalidParams("missing `id`".to_string()))
}

/// A handler that turns work away — the safe default when A2A is not wired up.
pub struct ClosedAgent {
    pub card: AgentCard,
}

#[async_trait::async_trait]
impl A2aHandler for ClosedAgent {
    fn agent_card(&self) -> AgentCard {
        self.card.clone()
    }

    async fn send(&self, _params: TaskSendParams) -> A2aResult<Task> {
        Err(A2aError::UnsupportedOperation(
            "this agent is not accepting tasks".to_string(),
        ))
    }

    async fn get(&self, id: &str) -> A2aResult<Task> {
        let mut task = Task::new(id);
        task.transition(TaskStatus::with_message(TaskState::Failed, "unknown task"));
        Ok(task)
    }

    async fn cancel(&self, id: &str) -> A2aResult<Task> {
        let mut task = Task::new(id);
        task.transition(TaskStatus::new(TaskState::Canceled));
        Ok(task)
    }
}
