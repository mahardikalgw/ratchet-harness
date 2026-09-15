use crate::{SandboxError, error::SandboxResult};
use std::path::{Path, PathBuf};

/// Enforces filesystem and execution boundaries.
#[derive(Debug, Clone, PartialEq)]
pub struct SandboxGuard {
    pub fs_scope: FileSystemScope,
    pub shell_scope: ShellScope,
    pub network_allowed: bool,
}

impl SandboxGuard {
    pub fn new(policy: super::policy::SandboxPolicy) -> Self {
        Self {
            fs_scope: FileSystemScope::new(policy.allowed_paths),
            shell_scope: ShellScope::new(policy.shell_allowlist),
            network_allowed: policy.network_allowed,
        }
    }

    pub fn check_read(&self, path: &Path) -> SandboxResult<()> {
        self.fs_scope.check_allowed(path)
    }

    pub fn check_write(&self, path: &Path) -> SandboxResult<()> {
        self.fs_scope.check_allowed(path)
    }

    pub fn check_shell(&self, command: &str) -> SandboxResult<()> {
        self.shell_scope.check_allowed(command)
    }

    pub fn check_network(&self) -> SandboxResult<()> {
        if self.network_allowed {
            Ok(())
        } else {
            Err(SandboxError::NetworkDenied)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileSystemScope {
    allowed_paths: Vec<PathBuf>,
}

impl FileSystemScope {
    pub fn new(allowed_paths: Vec<PathBuf>) -> Self {
        Self { allowed_paths }
    }

    pub fn check_allowed(&self, path: &Path) -> SandboxResult<()> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let allowed = self.allowed_paths.iter().any(|allowed| {
            let allowed_canon = allowed.canonicalize().unwrap_or_else(|_| allowed.clone());
            canonical.starts_with(&allowed_canon)
                || path.starts_with(allowed)
                || path
                    .to_string_lossy()
                    .starts_with(&allowed.to_string_lossy().to_string())
        });

        if allowed {
            Ok(())
        } else {
            Err(SandboxError::PathDenied {
                path: path.display().to_string(),
                allowed: self
                    .allowed_paths
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellScope {
    allowlist: Vec<String>,
}

impl ShellScope {
    pub fn new(allowlist: Vec<String>) -> Self {
        Self { allowlist }
    }

    pub fn check_allowed(&self, command: &str) -> SandboxResult<()> {
        let trimmed = command.trim();
        let allowed = self
            .allowlist
            .iter()
            .any(|allowed| trimmed.starts_with(allowed));

        if allowed || self.allowlist.is_empty() {
            Ok(())
        } else {
            Err(SandboxError::ShellDenied {
                command: command.to_string(),
            })
        }
    }
}

/// What the user decided about a proposed action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    /// Allow this one action.
    Approve,
    /// Allow this action, and remember it for the rest of the session.
    ApproveAlways,
    /// Deny this one action.
    Reject,
    /// Deny this action, and remember it for the rest of the session.
    RejectAlways,
}

impl ApprovalDecision {
    pub fn is_approved(self) -> bool {
        matches!(
            self,
            ApprovalDecision::Approve | ApprovalDecision::ApproveAlways
        )
    }

    pub fn is_sticky(self) -> bool {
        matches!(
            self,
            ApprovalDecision::ApproveAlways | ApprovalDecision::RejectAlways
        )
    }
}

/// Something that can decide whether a risky action may proceed.
///
/// Implementations must be non-blocking-safe to call from async code; the CLI
/// implementation uses `spawn_blocking` internally.
pub trait ApprovalHandler: Send + Sync {
    fn request(&self, action: &str, details: &str) -> ApprovalDecision;
}

/// Denies everything not already allow-listed. Used when there is no
/// interactive terminal (ACP mode, CI).
pub struct AutoDeny;

impl ApprovalHandler for AutoDeny {
    fn request(&self, _action: &str, _details: &str) -> ApprovalDecision {
        ApprovalDecision::Reject
    }
}

/// Approves everything. Used for `--yes` / `ApprovalPolicy::AutoApproveAll`.
pub struct AutoApprove;

impl ApprovalHandler for AutoApprove {
    fn request(&self, _action: &str, _details: &str) -> ApprovalDecision {
        ApprovalDecision::Approve
    }
}
