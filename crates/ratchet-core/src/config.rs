use ratchet_sandbox::policy::SandboxPolicy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Project-level `ratchet.toml` configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub project: ProjectSettings,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub providers: HashMap<String, ProviderSettings>,
    #[serde(default, skip_serializing_if = "RoutingSettings::is_default")]
    pub routing: RoutingSettings,
    #[serde(default)]
    pub sandbox: SandboxPolicy,
    #[serde(default, skip_serializing_if = "McpSettings::is_empty")]
    pub mcp: McpSettings,
    #[serde(default, skip_serializing_if = "DelegationSettings::is_default")]
    pub delegation: DelegationSettings,
    /// External plugins (custom tools and verification gates).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugins: Vec<ratchet_plugins::PluginManifest>,
    #[serde(
        default = "default_ratchet_dir",
        skip_serializing_if = "is_default_ratchet_dir"
    )]
    pub ratchet_dir: PathBuf,
}

fn default_ratchet_dir() -> PathBuf {
    PathBuf::from(".ratchet")
}

fn is_default_ratchet_dir(path: &PathBuf) -> bool {
    path == &PathBuf::from(".ratchet")
}

impl McpSettings {
    pub fn is_empty(&self) -> bool {
        self.servers.is_empty()
    }
}

/// Multi-agent delegation settings (PRD §8 P2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DelegationSettings {
    /// Run a reviewer agent over each task's changes.
    #[serde(default)]
    pub review: bool,
    /// How many times a rejected task is sent back for revision.
    #[serde(default = "default_review_rounds")]
    pub max_review_rounds: u32,
    /// Role -> provider name, e.g. `{ reviewer = "claude" }`.
    #[serde(default)]
    pub roles: HashMap<String, String>,
}

impl Default for DelegationSettings {
    fn default() -> Self {
        Self {
            review: false,
            max_review_rounds: default_review_rounds(),
            roles: HashMap::new(),
        }
    }
}

fn default_review_rounds() -> u32 {
    2
}

/// External MCP servers Ratchet should connect to as a client.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct McpSettings {
    #[serde(default)]
    pub servers: HashMap<String, McpServerConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectSettings {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderSettings {
    pub kind: String,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub extra_headers: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RoutingSettings {
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub planning_tasks: Option<String>,
    #[serde(default)]
    pub policy: RoutingPolicy,
}

impl RoutingSettings {
    pub fn is_default(&self) -> bool {
        self.default.is_none()
            && self.planning_tasks.is_none()
            && self.policy == RoutingPolicy::Fixed
    }
}

impl DelegationSettings {
    pub fn is_default(&self) -> bool {
        !self.review && self.roles.is_empty() && self.max_review_rounds == default_review_rounds()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RoutingPolicy {
    #[default]
    Fixed,
    CapabilityThenCost,
    CostOptimized,
    Fastest,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            project: ProjectSettings {
                name: "unnamed".to_string(),
                description: None,
            },
            providers: HashMap::new(),
            routing: RoutingSettings::default(),
            sandbox: SandboxPolicy::default(),
            mcp: McpSettings::default(),
            delegation: DelegationSettings::default(),
            plugins: Vec::new(),
            ratchet_dir: default_ratchet_dir(),
        }
    }
}

impl ProjectConfig {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// A minimal starting configuration.
    ///
    /// Deliberately configures **no** providers: inventing `claude`/`deepseek`
    /// entries the user does not have produces a config whose routing points
    /// at providers with no credentials, so the first `ratchet run` fails for
    /// a reason that is hard to see. `ratchet provider add` fills this in.
    pub fn scaffold(project_name: impl Into<String>) -> Self {
        Self {
            project: ProjectSettings {
                name: project_name.into(),
                description: None,
            },
            providers: HashMap::new(),
            routing: RoutingSettings::default(),
            sandbox: SandboxPolicy::default(),
            mcp: McpSettings::default(),
            delegation: DelegationSettings::default(),
            plugins: Vec::new(),
            ratchet_dir: default_ratchet_dir(),
        }
    }
}
