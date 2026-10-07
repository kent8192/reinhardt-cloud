//! Identity responsibility boundary for the Control Plane.

#[cfg(server)]
use reinhardt::app_config;

#[cfg(server)]
#[app_config(name = "identity", label = "identity")]
pub struct IdentityConfig;

pub mod urls;

#[cfg(server)]
pub mod models;
pub mod services;

#[cfg(server)]
pub mod commands;
#[cfg(server)]
pub mod middleware;
#[cfg(server)]
pub mod persistence;
#[cfg(server)]
pub mod server;

pub mod client;
pub mod functions;
