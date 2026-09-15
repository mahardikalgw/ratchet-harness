//! Plugin SDK for Ratchet.
//!
//! Plugins are external commands that speak a small JSON-over-stdio protocol.
//! Keeping them out-of-process means a plugin can be written in any language,
//! cannot corrupt the harness's memory, and is subject to the same sandbox
//! review as any other external tool.
//!
//! Two kinds are supported:
//!
//! - **`gate`** — an extra verification gate. Given the spec's acceptance
//!   criteria and evidence from the working tree, it returns a verdict per
//!   criterion. This is the piece MCP does not cover: MCP provides tools, not
//!   verification semantics.
//! - **`tool`** — an extra tool the agent may call. Advertised via `describe`,
//!   invoked via `tool_call`.
//!
//! Each invocation is a one-shot process: a single JSON request on stdin and a
//! single JSON response on stdout. That keeps plugin crashes isolated and makes
//! plugins trivially testable.

pub mod error;
pub mod host;
pub mod protocol;

pub use error::{PluginError, PluginResult};
pub use host::{PluginHost, PluginInvocation};
pub use protocol::{
    GateCriterion, GateRequest, GateResult, GateStatus, PluginKind, PluginManifest,
    ToolCallRequest, ToolCallResponse, ToolDescriptor,
};
