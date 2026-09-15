use serde::{Deserialize, Serialize};

pub type TaskId = String;

/// Lifecycle states a delegated task moves through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    Submitted,
    Working,
    /// Waiting on a human (e.g. an approval gate).
    InputRequired,
    Completed,
    Canceled,
    Failed,
    Unknown,
}

impl TaskState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskState::Completed | TaskState::Canceled | TaskState::Failed
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskStatus {
    pub state: TaskState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<Message>,
    #[serde(default)]
    pub timestamp: Option<String>,
}

impl TaskStatus {
    pub fn new(state: TaskState) -> Self {
        Self {
            state,
            message: None,
            timestamp: Some(chrono_like_timestamp()),
        }
    }

    pub fn with_message(state: TaskState, text: impl Into<String>) -> Self {
        Self {
            state,
            message: Some(Message::agent_text(text)),
            timestamp: Some(chrono_like_timestamp()),
        }
    }

    pub fn terminal(&self) -> bool {
        self.state.is_terminal()
    }
}

/// An inbound or outbound message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub parts: Vec<Part>,
}

impl Message {
    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            parts: vec![Part::Text { text: text.into() }],
        }
    }

    pub fn agent_text(text: impl Into<String>) -> Self {
        Self {
            role: "agent".to_string(),
            parts: vec![Part::Text { text: text.into() }],
        }
    }

    /// Concatenate all text parts.
    pub fn text(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| match p {
                Part::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Part {
    Text { text: String },
    Data { data: serde_json::Value },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Artifact {
    #[serde(default)]
    pub name: Option<String>,
    pub parts: Vec<Part>,
}

/// A unit of delegated work.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub status: TaskStatus,
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
    #[serde(default)]
    pub history: Vec<TaskStatusUpdate>,
}

impl Task {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            session_id: None,
            status: TaskStatus::new(TaskState::Submitted),
            artifacts: Vec::new(),
            history: Vec::new(),
        }
    }

    /// Apply a status change, recording it in the history.
    pub fn transition(&mut self, status: TaskStatus) {
        self.history.push(TaskStatusUpdate {
            status: status.clone(),
        });
        self.status = status;
    }

    pub fn add_text_artifact(&mut self, name: impl Into<String>, text: impl Into<String>) {
        self.artifacts.push(Artifact {
            name: Some(name.into()),
            parts: vec![Part::Text { text: text.into() }],
        });
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskStatusUpdate {
    pub status: TaskStatus,
}

/// Parameters for `tasks/send`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskSendParams {
    /// Caller-supplied task id, so retries can be idempotent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub message: Message,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl TaskSendParams {
    /// Extract the spec id this task refers to, if the caller supplied one.
    pub fn spec_id(&self) -> Option<String> {
        if let Some(spec) = self.metadata.get("spec_id").and_then(|v| v.as_str()) {
            return Some(spec.to_string());
        }
        // Fall back to a `spec:<id>` token in the text.
        self.message
            .text()
            .split_whitespace()
            .find_map(|token| token.strip_prefix("spec:").map(|s| s.to_string()))
    }

    /// Which skill the caller is asking for.
    pub fn skill(&self) -> Option<String> {
        self.metadata
            .get("skill")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }
}

/// Minimal RFC3339-ish timestamp without pulling in a date library.
fn chrono_like_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}
