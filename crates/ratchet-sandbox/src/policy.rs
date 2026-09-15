use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level sandbox configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SandboxPolicy {
    /// Read and write.
    #[serde(default = "default_allowed_paths")]
    pub allowed_paths: Vec<PathBuf>,
    /// Read only. Used for agent instruction directories (`.agents`, ...),
    /// which are useful context but must not be rewritten by the agent itself.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub read_only_paths: Vec<PathBuf>,
    #[serde(default = "default_shell_allowlist")]
    pub shell_allowlist: Vec<String>,
    #[serde(default)]
    pub network_allowed: bool,
    #[serde(default)]
    pub approval_policy: ApprovalPolicy,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self {
            allowed_paths: default_allowed_paths(),
            read_only_paths: Vec::new(),
            shell_allowlist: default_shell_allowlist(),
            network_allowed: false,
            approval_policy: ApprovalPolicy::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalPolicy {
    #[default]
    Interactive,
    AutoApproveSafe,
    AutoApproveAll,
    DenyAll,
}

fn default_allowed_paths() -> Vec<PathBuf> {
    vec![
        PathBuf::from("src"),
        PathBuf::from("tests"),
        PathBuf::from("docs"),
        PathBuf::from(".ratchet"),
    ]
}

fn default_shell_allowlist() -> Vec<String> {
    vec![
        "cargo test".to_string(),
        "cargo fmt".to_string(),
        "cargo clippy".to_string(),
        "cargo build".to_string(),
        "cargo check".to_string(),
        "npm test".to_string(),
        "npm run lint".to_string(),
        "pytest".to_string(),
        "python -m pytest".to_string(),
        "git status".to_string(),
        "git diff".to_string(),
        "git log".to_string(),
    ]
}
