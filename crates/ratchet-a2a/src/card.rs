use serde::{Deserialize, Serialize};

/// Served at `/.well-known/agent.json` so peers can discover what this agent
/// can do without out-of-band coordination.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentCard {
    pub name: String,
    pub description: String,
    /// Base URL peers should call for JSON-RPC requests.
    pub url: String,
    pub version: String,
    #[serde(default)]
    pub capabilities: AgentCapabilities,
    #[serde(default)]
    pub default_input_modes: Vec<String>,
    #[serde(default)]
    pub default_output_modes: Vec<String>,
    #[serde(default)]
    pub skills: Vec<AgentSkill>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AgentCapabilities {
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub push_notifications: bool,
    #[serde(default)]
    pub state_transition_history: bool,
}

/// A discrete capability a peer can ask for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentSkill {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub examples: Vec<String>,
}

impl AgentCard {
    /// The card Ratchet publishes: one skill per spec-driven operation.
    pub fn ratchet(url: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: "Ratchet".to_string(),
            description: "Spec-driven engineering harness. Delegates work from versioned specs, \
                 executes it with tools, and reports verification results."
                .to_string(),
            url: url.into(),
            version: version.into(),
            capabilities: AgentCapabilities {
                streaming: false,
                push_notifications: false,
                state_transition_history: true,
            },
            default_input_modes: vec!["text/plain".to_string()],
            default_output_modes: vec!["text/plain".to_string()],
            skills: vec![
                AgentSkill {
                    id: "run-spec".to_string(),
                    name: "Execute a spec".to_string(),
                    description: "Plan and execute a spec's task graph, then verify it."
                        .to_string(),
                    tags: vec!["code".to_string(), "spec".to_string()],
                    examples: vec!["Implement the slugify spec".to_string()],
                },
                AgentSkill {
                    id: "plan-spec".to_string(),
                    name: "Plan a spec".to_string(),
                    description: "Produce a task graph for a spec without executing it."
                        .to_string(),
                    tags: vec!["planning".to_string()],
                    examples: vec!["Plan the slugify spec".to_string()],
                },
                AgentSkill {
                    id: "verify-spec".to_string(),
                    name: "Verify a spec".to_string(),
                    description: "Check the working tree against a spec's acceptance criteria."
                        .to_string(),
                    tags: vec!["verification".to_string()],
                    examples: vec!["Verify the slugify spec".to_string()],
                },
            ],
        }
    }
}
