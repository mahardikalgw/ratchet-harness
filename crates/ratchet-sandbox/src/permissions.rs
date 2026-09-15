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
            fs_scope: FileSystemScope::new(policy.allowed_paths, policy.read_only_paths),
            shell_scope: ShellScope::new(policy.shell_allowlist),
            network_allowed: policy.network_allowed,
        }
    }

    pub fn check_read(&self, path: &Path) -> SandboxResult<()> {
        self.fs_scope.check_read(path)
    }

    pub fn check_write(&self, path: &Path) -> SandboxResult<()> {
        self.fs_scope.check_write(path)
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

/// Path scope with a read/write distinction.
///
/// Skills, tooling configuration, and other agent instructions live in the
/// repository and are useful context, but letting the agent *write* them means
/// it can rewrite its own instructions — a self-modification hole. Those
/// directories are readable and not writable.
#[derive(Debug, Clone, PartialEq)]
pub struct FileSystemScope {
    writable: Vec<PathBuf>,
    readable: Vec<PathBuf>,
}

impl FileSystemScope {
    pub fn new(writable: Vec<PathBuf>, read_only: Vec<PathBuf>) -> Self {
        let mut readable = writable.clone();
        for path in read_only {
            if !readable.contains(&path) {
                readable.push(path);
            }
        }
        Self { writable, readable }
    }

    /// Write-only scope (no read-only extras).
    pub fn writable_only(writable: Vec<PathBuf>) -> Self {
        Self::new(writable, Vec::new())
    }

    pub fn check_read(&self, path: &Path) -> SandboxResult<()> {
        if self.allows(&self.readable, path) {
            Ok(())
        } else {
            Err(SandboxError::PathDenied {
                path: path.display().to_string(),
                allowed: self.describe(&self.readable),
            })
        }
    }

    pub fn check_write(&self, path: &Path) -> SandboxResult<()> {
        if self.allows(&self.writable, path) {
            return Ok(());
        }
        // Distinguish "outside everything" from "readable but not writable",
        // because the fix is different for each.
        if self.allows(&self.readable, path) {
            return Err(SandboxError::PathReadOnly {
                path: path.display().to_string(),
            });
        }
        Err(SandboxError::PathDenied {
            path: path.display().to_string(),
            allowed: self.describe(&self.writable),
        })
    }

    fn allows(&self, scope: &[PathBuf], path: &Path) -> bool {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        scope.iter().any(|allowed| {
            let allowed_canon = allowed.canonicalize().unwrap_or_else(|_| allowed.clone());
            canonical.starts_with(&allowed_canon)
                || path.starts_with(allowed)
                || path
                    .to_string_lossy()
                    .starts_with(&allowed.to_string_lossy().to_string())
        })
    }

    fn describe(&self, scope: &[PathBuf]) -> String {
        scope
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
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
    Approve,
    ApproveAlways,
    Reject,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_paths_are_readable() {
        let scope = FileSystemScope::new(
            vec![PathBuf::from("/work/src")],
            vec![PathBuf::from("/work/.agents")],
        );
        assert!(
            scope
                .check_read(Path::new("/work/.agents/skills/x/SKILL.md"))
                .is_ok()
        );
    }

    #[test]
    fn read_only_paths_are_not_writable() {
        let scope = FileSystemScope::new(
            vec![PathBuf::from("/work/src")],
            vec![PathBuf::from("/work/.agents")],
        );

        let error = scope
            .check_write(Path::new("/work/.agents/skills/x/SKILL.md"))
            .unwrap_err();

        // The error must say *why*, so the agent can pick a different action.
        match error {
            SandboxError::PathReadOnly { path } => assert!(path.contains(".agents")),
            other => panic!("expected PathReadOnly, got {other:?}"),
        }
    }

    #[test]
    fn writable_paths_stay_writable() {
        let scope = FileSystemScope::writable_only(vec![PathBuf::from("/work/src")]);
        assert!(scope.check_write(Path::new("/work/src/lib.rs")).is_ok());
        assert!(scope.check_read(Path::new("/work/src/lib.rs")).is_ok());
    }

    #[test]
    fn outside_paths_are_denied_for_both() {
        let scope = FileSystemScope::new(
            vec![PathBuf::from("/work/src")],
            vec![PathBuf::from("/work/.agents")],
        );
        assert!(scope.check_read(Path::new("/etc/passwd")).is_err());
        assert!(scope.check_write(Path::new("/etc/passwd")).is_err());
        assert!(matches!(
            scope.check_write(Path::new("/etc/passwd")),
            Err(SandboxError::PathDenied { .. })
        ));
    }

    #[test]
    fn a_path_listed_both_ways_is_writable() {
        // Writing wins: a directory in `allowed_paths` is never demoted.
        let scope = FileSystemScope::new(
            vec![PathBuf::from("/work/src")],
            vec![PathBuf::from("/work/src")],
        );
        assert!(scope.check_write(Path::new("/work/src/lib.rs")).is_ok());
    }

    #[test]
    fn shell_scope_enforces_the_allow_list() {
        let scope = ShellScope::new(vec!["cargo test".to_string()]);
        assert!(scope.check_allowed("cargo test --all").is_ok());
        assert!(scope.check_allowed("rm -rf /").is_err());
    }

    #[test]
    fn empty_allow_list_permits_everything() {
        // Preserved behaviour: an unconfigured list means "no restriction".
        let scope = ShellScope::new(vec![]);
        assert!(scope.check_allowed("anything").is_ok());
    }
}
