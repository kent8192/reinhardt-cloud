//! Project responsibility boundary for the Control Plane.

#[cfg(server)]
use reinhardt::app_config;

#[cfg(server)]
#[app_config(name = "project", label = "project")]
pub struct ProjectConfig;

pub mod client;
pub mod urls;

#[cfg(server)]
pub mod models;
#[cfg(server)]
pub mod server;
pub mod services;

#[cfg(server)]
pub mod persistence;

pub mod functions;
