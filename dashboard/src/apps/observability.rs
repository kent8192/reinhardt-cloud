//! Observability responsibility boundary for the Control Plane.

#[cfg(server)]
use reinhardt::app_config;

#[cfg(server)]
#[app_config(name = "observability", label = "observability")]
pub struct ObservabilityConfig;

pub mod urls;

#[cfg(server)]
pub mod models;
#[cfg(server)]
pub mod server;
pub mod services;
