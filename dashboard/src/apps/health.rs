//! Health application module.
//!
//! Exposes an unauthenticated `/api/healthz/` endpoint for Kubernetes
//! liveness and readiness probes. The endpoint verifies the health of
//! the database connection and the gRPC channel. The `urls` submodule
//! is cross-target so the typed SPA accessor reaches it on wasm.

#[cfg(native)]
use reinhardt::app_config;

#[cfg(native)]
pub mod models;
#[cfg(native)]
pub mod serializers;
#[cfg(native)]
pub mod tests;
pub mod urls;
#[cfg(native)]
pub mod views;

#[cfg(native)]
#[app_config(name = "health", label = "health")]
pub struct HealthConfig;
