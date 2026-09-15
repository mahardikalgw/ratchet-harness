use crate::{ToolContext, error::ToolResult};
use serde_json::Value;
use std::path::Path;

/// A recognised test runner.
///
/// This is only a *fallback*: the project's configured `test_command` wins when
/// one is set, which is what lets Ratchet drive a language it has never heard
/// of. Detection exists so a fresh checkout works before anyone configures it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestRunnerKind {
    Cargo,
    Npm,
    Pnpm,
    Yarn,
    Pytest,
    Go,
    Ruby,
    Maven,
    Gradle,
    Php,
    Make,
    /// No runner recognised; the caller must supply a command.
    Unknown,
}

impl TestRunnerKind {
    pub fn label(&self) -> &'static str {
        match self {
            TestRunnerKind::Cargo => "cargo",
            TestRunnerKind::Npm => "npm",
            TestRunnerKind::Pnpm => "pnpm",
            TestRunnerKind::Yarn => "yarn",
            TestRunnerKind::Pytest => "pytest",
            TestRunnerKind::Go => "go",
            TestRunnerKind::Ruby => "rspec",
            TestRunnerKind::Maven => "maven",
            TestRunnerKind::Gradle => "gradle",
            TestRunnerKind::Php => "phpunit",
            TestRunnerKind::Make => "make",
            TestRunnerKind::Unknown => "unknown",
        }
    }

    /// The command that runs this project's tests.
    pub fn command(&self, filter: Option<&str>) -> Option<String> {
        let filter = filter.map(str::trim).filter(|f| !f.is_empty());
        let cmd = match self {
            TestRunnerKind::Cargo => match filter {
                Some(f) => format!("cargo test {f}"),
                None => "cargo test".to_string(),
            },
            TestRunnerKind::Npm => match filter {
                Some(f) => format!("npm test -- {f}"),
                None => "npm test".to_string(),
            },
            TestRunnerKind::Pnpm => match filter {
                Some(f) => format!("pnpm test {f}"),
                None => "pnpm test".to_string(),
            },
            TestRunnerKind::Yarn => match filter {
                Some(f) => format!("yarn test {f}"),
                None => "yarn test".to_string(),
            },
            TestRunnerKind::Pytest => match filter {
                Some(f) => format!("pytest {f}"),
                None => "pytest".to_string(),
            },
            TestRunnerKind::Go => match filter {
                Some(f) => format!("go test ./... -run {f}"),
                None => "go test ./...".to_string(),
            },
            TestRunnerKind::Ruby => match filter {
                Some(f) => format!("bundle exec rspec {f}"),
                None => "bundle exec rspec".to_string(),
            },
            TestRunnerKind::Maven => match filter {
                Some(f) => format!("mvn test -Dtest={f}"),
                None => "mvn test".to_string(),
            },
            TestRunnerKind::Gradle => match filter {
                Some(f) => format!("./gradlew test --tests {f}"),
                None => "./gradlew test".to_string(),
            },
            TestRunnerKind::Php => match filter {
                Some(f) => format!("vendor/bin/phpunit {f}"),
                None => "vendor/bin/phpunit".to_string(),
            },
            TestRunnerKind::Make => "make test".to_string(),
            TestRunnerKind::Unknown => return None,
        };
        Some(cmd)
    }
}

pub struct TestRunner {
    pub kind: TestRunnerKind,
    /// Explicit command, when the project configured one.
    pub command: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub summary: String,
}

impl TestRunner {
    /// Choose a runner for this context.
    ///
    /// A configured command always wins — that is the escape hatch for any
    /// language or custom harness Ratchet does not recognise.
    pub fn for_context(ctx: &ToolContext) -> Self {
        match &ctx.test_command {
            Some(command) if !command.trim().is_empty() => Self {
                kind: TestRunnerKind::Unknown,
                command: Some(command.clone()),
            },
            _ => Self::detect(&ctx.cwd),
        }
    }

    /// Detect from manifest files. Never fails; unknown is a valid answer.
    pub fn detect(cwd: &Path) -> Self {
        let has = |name: &str| cwd.join(name).exists();

        let kind = if has("Cargo.toml") {
            TestRunnerKind::Cargo
        } else if has("pnpm-lock.yaml") {
            TestRunnerKind::Pnpm
        } else if has("yarn.lock") {
            TestRunnerKind::Yarn
        } else if has("package.json") {
            TestRunnerKind::Npm
        } else if has("pyproject.toml") || has("pytest.ini") || has("setup.py") || has("tox.ini") {
            TestRunnerKind::Pytest
        } else if has("go.mod") {
            TestRunnerKind::Go
        } else if has("Gemfile") {
            TestRunnerKind::Ruby
        } else if has("pom.xml") {
            TestRunnerKind::Maven
        } else if has("build.gradle") || has("build.gradle.kts") {
            TestRunnerKind::Gradle
        } else if has("composer.json") {
            TestRunnerKind::Php
        } else if has("Makefile") {
            TestRunnerKind::Make
        } else {
            TestRunnerKind::Unknown
        };

        Self {
            kind,
            command: None,
        }
    }

    /// The command that will actually run.
    pub fn resolved_command(&self, filter: Option<&str>) -> Option<String> {
        match &self.command {
            Some(command) => {
                // Append a filter to a configured command when it takes one.
                match filter.map(str::trim).filter(|f| !f.is_empty()) {
                    Some(f) => Some(format!("{command} {f}")),
                    None => Some(command.clone()),
                }
            }
            None => self.kind.command(filter),
        }
    }

    pub async fn execute(&self, ctx: &ToolContext, filter: Option<&str>) -> ToolResult<Value> {
        let command = self.resolved_command(filter).ok_or_else(|| {
            crate::error::ToolError::Execution(
                "no test runner could be detected — set `test_command` under [project] \
                 in ratchet.toml"
                    .to_string(),
            )
        })?;

        tracing::info!(command = %command, runner = %self.kind.label(), "running tests");

        let shell = crate::shell::ShellExec;
        shell
            .execute(ctx, &command, Some(std::time::Duration::from_secs(600)))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(cwd: &Path, test_command: Option<&str>) -> ToolContext {
        ToolContext {
            cwd: cwd.to_path_buf(),
            sandbox: ratchet_sandbox::SandboxGuard::new(
                ratchet_sandbox::policy::SandboxPolicy::default(),
            ),
            registry: crate::registry::ToolRegistry::new(),
            test_command: test_command.map(|s| s.to_string()),
        }
    }

    #[test]
    fn detects_each_ecosystem() {
        for (manifest, expected) in [
            ("Cargo.toml", TestRunnerKind::Cargo),
            ("package.json", TestRunnerKind::Npm),
            ("yarn.lock", TestRunnerKind::Yarn),
            ("pnpm-lock.yaml", TestRunnerKind::Pnpm),
            ("go.mod", TestRunnerKind::Go),
            ("Gemfile", TestRunnerKind::Ruby),
            ("pom.xml", TestRunnerKind::Maven),
            ("build.gradle", TestRunnerKind::Gradle),
            ("composer.json", TestRunnerKind::Php),
            ("Makefile", TestRunnerKind::Make),
        ] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join(manifest), "").unwrap();
            assert_eq!(
                TestRunner::detect(dir.path()).kind,
                expected,
                "manifest {manifest}"
            );
        }
    }

    #[test]
    fn lockfiles_disambiguate_the_node_package_manager() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        assert_eq!(TestRunner::detect(dir.path()).kind, TestRunnerKind::Pnpm);
    }

    #[test]
    fn unknown_project_reports_unknown() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(TestRunner::detect(dir.path()).kind, TestRunnerKind::Unknown);
        assert!(
            TestRunner::detect(dir.path())
                .resolved_command(None)
                .is_none()
        );
    }

    #[test]
    fn configured_command_overrides_detection() {
        // This is what makes Ratchet language-agnostic: any command, any language.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();

        let ctx = ctx(dir.path(), Some("./scripts/verify.sh"));
        let runner = TestRunner::for_context(&ctx);
        assert_eq!(
            runner.resolved_command(None).as_deref(),
            Some("./scripts/verify.sh")
        );
    }

    #[test]
    fn filters_are_appended_per_runner() {
        assert_eq!(
            TestRunnerKind::Cargo.command(Some("auth")).as_deref(),
            Some("cargo test auth")
        );
        assert_eq!(
            TestRunnerKind::Go.command(Some("TestAuth")).as_deref(),
            Some("go test ./... -run TestAuth")
        );
        assert_eq!(
            TestRunnerKind::Maven.command(Some("AuthTest")).as_deref(),
            Some("mvn test -Dtest=AuthTest")
        );
        assert_eq!(
            TestRunnerKind::Gradle.command(Some("AuthTest")).as_deref(),
            Some("./gradlew test --tests AuthTest")
        );
    }

    #[test]
    fn empty_filter_is_ignored() {
        assert_eq!(
            TestRunnerKind::Cargo.command(Some("   ")).as_deref(),
            Some("cargo test")
        );
    }

    #[test]
    fn every_known_runner_has_a_command() {
        for kind in [
            TestRunnerKind::Cargo,
            TestRunnerKind::Npm,
            TestRunnerKind::Pnpm,
            TestRunnerKind::Yarn,
            TestRunnerKind::Pytest,
            TestRunnerKind::Go,
            TestRunnerKind::Ruby,
            TestRunnerKind::Maven,
            TestRunnerKind::Gradle,
            TestRunnerKind::Php,
            TestRunnerKind::Make,
        ] {
            assert!(kind.command(None).is_some(), "{kind:?} has no command");
        }
    }

    #[tokio::test]
    async fn unknown_runner_without_a_command_is_an_actionable_error() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ctx(dir.path(), None);
        let runner = TestRunner::for_context(&ctx);

        let error = runner.execute(&ctx, None).await.unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("test_command"),
            "unhelpful error: {message}"
        );
    }
}
