//! Deployment responsibility boundary for the Control Plane.

#[cfg(server)]
use reinhardt::app_config;

#[cfg(server)]
#[app_config(name = "deployment", label = "deployment")]
pub struct DeploymentConfig;

pub mod urls;

#[cfg(server)]
pub mod models;
pub mod services;

#[cfg(server)]
pub mod persistence;

#[cfg(server)]
pub(crate) mod read;

pub mod client;
pub mod functions;
