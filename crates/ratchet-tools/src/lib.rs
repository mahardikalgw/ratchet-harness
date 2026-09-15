pub mod error;
pub mod executor;
pub mod fs;
pub mod git;
pub mod path;
pub mod registry;
pub mod search;
pub mod shell;
pub mod test_runner;

pub use error::{ToolError, ToolResult};
pub use executor::{ToolContext, ToolExecutor};
pub use fs::{FilePatch, FileRead, FileWrite};
pub use git::{GitCommit, GitDiff, GitStatus};
pub use path::{resolve_path, string_arg};
pub use registry::{ToolDefinition, ToolRegistry};
pub use search::{repo_map, Grep, ListDir};
pub use shell::ShellExec;
pub use test_runner::{TestResult, TestRunner, TestRunnerKind};
