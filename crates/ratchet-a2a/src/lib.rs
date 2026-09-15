//! Agent-to-Agent (A2A) protocol surface.
//!
//! PRD §8 P2: "A2A exposure of a Ratchet task as a callable peer agent."
//!
//! This crate models the wire types and nothing else — no transport, no
//! runtime — so the protocol can be reused by any server implementation and
//! tested in isolation.

pub mod card;
pub mod error;
pub mod task;

pub use card::{AgentCapabilities, AgentCard, AgentSkill};
pub use error::{A2aError, A2aResult};
pub use task::{
    Artifact, Message, Part, Task, TaskId, TaskSendParams, TaskState, TaskStatus, TaskStatusUpdate,
};
