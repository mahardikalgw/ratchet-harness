use thiserror::Error;

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("spec error: {0}")]
    Spec(#[from] ratchet_spec::SpecError),

    #[error("provider error: {0}")]
    Provider(String),

    #[error("tool error: {0}")]
    Tool(String),

    #[error("sandbox error: {0}")]
    Sandbox(String),

    #[error("memory error: {0}")]
    Memory(#[from] ratchet_memory::MemoryError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("observability error: {0}")]
    Observability(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("execution error: {0}")]
    Execution(String),

    #[error("approval required: {action}")]
    ApprovalRequired { action: String },

    #[error("task failed: {task_id} — {reason}")]
    TaskFailed { task_id: String, reason: String },

    #[error("no provider available for model: {0}")]
    NoProvider(String),

    #[error("plan generation failed: {0}")]
    PlanGeneration(String),

    #[error("verification failed: {0}")]
    Verification(String),
}
