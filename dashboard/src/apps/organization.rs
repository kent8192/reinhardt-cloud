//! Organization responsibility boundary for the Control Plane.

#[cfg(server)]
use reinhardt::app_config;

#[cfg(server)]
#[app_config(name = "organization", label = "organization")]
pub struct OrganizationConfig;

pub mod urls;

#[cfg(server)]
pub mod models;
pub mod services;

#[cfg(server)]
pub mod persistence;
#[cfg(server)]
pub mod server;
