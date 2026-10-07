//! Source responsibility boundary for the Control Plane.

#[cfg(server)]
use reinhardt::app_config;

#[cfg(server)]
#[app_config(name = "source", label = "source")]
pub struct SourceConfig;

pub mod urls;

#[cfg(server)]
pub mod models;
pub mod services;
