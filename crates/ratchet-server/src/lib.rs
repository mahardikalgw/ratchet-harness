//! Minimal HTTP surface for Ratchet.
//!
//! Deliberately dependency-light: a small HTTP/1.1 implementation over Tokio,
//! enough to serve a local team dashboard and the A2A JSON-RPC endpoint. This
//! is not a general-purpose web server — it handles the traffic it is designed
//! for (local GETs and small JSON POSTs) and rejects anything ambiguous.

pub mod a2a;
pub mod dashboard;
pub mod http;
pub mod server;

pub use a2a::A2aHandler;
pub use dashboard::{DashboardSource, NullDashboard};
pub use http::{HttpRequest, HttpResponse};
pub use server::RatchetServer;
