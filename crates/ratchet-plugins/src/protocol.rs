use serde::{Deserialize, Serialize};

/// What a plugin provides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginKind {
    /// An extra verification gate.
    Gate,
    /// An extra tool the agent can call.
    Tool,
}

/// Declaration of a plugin, as written in `ratchet.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub kind: PluginKind,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Gate plugins only: restrict this gate to specific acceptance criteria.
    /// Empty means "apply to every criterion".
    #[serde(default)]
    pub criteria: Vec<String>,
    /// Per-invocation timeout.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 {
    60
}

impl PluginManifest {
    /// Does this gate apply to the given criterion id?
    pub fn applies_to(&self, criterion_id: &str) -> bool {
        self.criteria.is_empty() || self.criteria.iter().any(|c| c == criterion_id)
    }
}

// ----- Requests -----

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PluginRequest {
    /// Ask a tool plugin what tools it provides.
    Describe,
    /// Invoke a tool.
    ToolCall(ToolCallRequest),
    /// Ask a gate plugin to judge acceptance criteria.
    Gate(GateRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateRequest {
    pub spec_id: String,
    /// Acceptance criteria this plugin is being asked about.
    pub criteria: Vec<GateCriterion>,
    #[serde(default)]
    pub changed_files: Vec<String>,
    #[serde(default)]
    pub test_passed: Option<bool>,
    #[serde(default)]
    pub test_output: String,
    /// Working directory the run is happening in.
    #[serde(default)]
    pub working_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateCriterion {
    pub id: String,
    pub description: String,
}

// ----- Responses -----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescribeResponse {
    #[serde(default)]
    pub tools: Vec<ToolDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResponse {
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateResponse {
    #[serde(default)]
    pub results: Vec<GateResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateResult {
    pub criterion_id: String,
    pub status: GateStatus,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GateStatus {
    Passed,
    Failed,
    /// The gate could not decide; surfaced to the human rather than guessed.
    Manual,
}

impl GateStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            GateStatus::Passed => "✅",
            GateStatus::Failed => "❌",
            GateStatus::Manual => "🟡",
        }
    }
}
