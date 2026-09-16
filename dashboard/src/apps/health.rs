//! Health application module.
//!
//! Exposes the unauthenticated `/api/healthz/` infrastructure endpoint.

#[cfg(server)]
use reinhardt::app_config;

#[cfg(server)]
pub mod models;
#[cfg(server)]
pub mod serializers;
#[cfg(server)]
pub mod server_urls;
#[cfg(server)]
pub mod tests;
pub mod urls;

#[cfg(server)]
#[app_config(name = "health", label = "health")]
pub struct HealthConfig;
