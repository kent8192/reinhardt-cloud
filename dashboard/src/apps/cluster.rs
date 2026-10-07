//! Cluster responsibility boundary for the Control Plane.

#[cfg(server)]
use reinhardt::app_config;

#[cfg(server)]
#[app_config(name = "cluster", label = "cluster")]
pub struct ClusterConfig;

pub mod urls;

#[cfg(server)]
pub mod models;
pub mod services;
