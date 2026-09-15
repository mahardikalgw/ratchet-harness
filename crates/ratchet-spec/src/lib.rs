pub mod error;
pub mod extract;
pub mod format;
pub mod parser;
pub mod schema;
pub mod task;
pub mod validator;

pub use error::{SpecError, SpecResult};
pub use extract::SpecExtractor;
pub use format::{SpecFile, SpecFrontmatter, SpecSection};
pub use parser::SpecParser;
pub use schema::{AcceptanceCriterion, Constraint, Goal, Plan, TaskGraph, TaskNode, VerificationStep};
pub use task::{Task, TaskId, TaskStatus};
pub use validator::SpecValidator;
