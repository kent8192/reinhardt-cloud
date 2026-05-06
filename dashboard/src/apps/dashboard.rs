//! Dashboard application module.
//!
//! Owns the project-level SPA route table — the home/clusters/deployments
//! pages reachable from the top-level navigation. Views are pulled from
//! `crate::client` until each section grows its own per-app implementation.

use reinhardt::app_config;

pub mod urls;
pub mod views;

#[app_config(name = "dashboard", label = "dashboard")]
pub struct DashboardConfig;
