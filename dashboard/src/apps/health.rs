//! Health application module.
//!
//! Exposes an unauthenticated `/api/healthz/` endpoint for Kubernetes
//! liveness and readiness probes. The endpoint verifies the health of
//! the database connection and the gRPC channel.

use reinhardt::app_config;

pub mod models;
pub mod serializers;
pub mod tests;
pub mod urls;
pub mod views;

#[app_config(name = "health", label = "health")]
pub struct HealthConfig;
