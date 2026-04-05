//! Service trait definitions for the Reinhardt Cloud platform.
//!
//! These traits define the contract for core platform services (auth, build,
//! cluster agent, log). Implementations may be backed by local logic, gRPC
//! clients, or mock objects for testing.

pub mod auth;
pub mod build;
pub mod cluster_agent;
pub mod log;

pub use auth::AuthService;
pub use build::BuildService;
pub use cluster_agent::ClusterAgentService;
pub use log::LogService;
