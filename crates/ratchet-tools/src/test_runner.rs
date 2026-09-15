use crate::{error::ToolResult, ToolContext};
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TestRunnerKind {
    Cargo,
    Npm,
    Pytest,
    Unknown,
}

pub struct TestRunner {
    pub kind: TestRunnerKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub summary: String,
}

impl TestRunner {
    pub fn detect(cwd: &Path) -> ToolResult<Self> {
        if cwd.join("Cargo.toml").exists() {
            Ok(Self { kind: TestRunnerKind::Cargo })
        } else if cwd.join("package.json").exists() {
            Ok(Self { kind: TestRunnerKind::Npm })
        } else if cwd.join("pytest.ini").exists() || cwd.join("setup.py").exists() || cwd.join("pyproject.toml").exists() {
            Ok(Self { kind: TestRunnerKind::Pytest })
        } else {
            Ok(Self { kind: TestRunnerKind::Unknown })
        }
    }

    pub async fn execute(&self, ctx: &ToolContext, filter: Option<&str>) -> ToolResult<Value> {
        let command = match self.kind {
            TestRunnerKind::Cargo => {
                let mut cmd = "cargo test".to_string();
                if let Some(f) = filter {
                    cmd.push(' ');
                    cmd.push_str(f);
                }
                cmd
            }
            TestRunnerKind::Npm => {
                let cmd = "npm test".to_string();
                let _ = filter; // npm test doesn't easily support filters
                cmd
            }
            TestRunnerKind::Pytest => {
                let mut cmd = "pytest".to_string();
                if let Some(f) = filter {
                    cmd.push(' ');
                    cmd.push_str(f);
                }
                cmd
            }
            TestRunnerKind::Unknown => {
                return Err(crate::error::ToolError::Execution(
                    "could not detect test runner".into(),
                ));
            }
        };

        let shell = crate::shell::ShellExec;
        shell.execute(ctx, &command, Some(std::time::Duration::from_secs(300))).await
    }
}
