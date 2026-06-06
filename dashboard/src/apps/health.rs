//! Health application module.
//!
//! Exposes the unauthenticated `/api/healthz/` infrastructure endpoint.

#[cfg(native)]
use reinhardt::app_config;

#[cfg(native)]
pub mod models;
#[cfg(native)]
pub mod serializers;
#[cfg(native)]
pub mod server_urls;
#[cfg(native)]
pub mod tests;
pub mod urls;

#[cfg(native)]
#[app_config(name = "health", label = "health")]
pub struct HealthConfig;
