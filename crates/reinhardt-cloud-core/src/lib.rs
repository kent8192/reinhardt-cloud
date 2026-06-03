//! Core business logic for the Reinhardt Cloud PaaS platform.
//!
//! This crate provides framework-agnostic domain services, error types,
//! and authentication utilities. It has no dependency on reinhardt or
//! any web framework.

pub mod auth;
pub mod error;
pub mod inference;
pub mod infrastructure_derivation;
pub mod mocks;
pub mod pagination;
pub mod plugin;
pub mod services;
pub mod traits;

pub use error::ApiError;
pub use mocks::{MockAuthService, MockBuildService, MockClusterAgentService, MockLogService};
pub use pagination::{PaginatedResponse, PaginationParams};
pub use traits::{AuthService, BuildService, ClusterAgentService, LogService};
