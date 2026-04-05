//! Clusters application module.
//!
//! Provides endpoints for Kubernetes cluster management.

use reinhardt::app_config;

pub mod admin;
pub mod models;
pub mod serializers;
pub mod tests;
pub mod urls;
pub mod views;

#[app_config(name = "clusters", label = "clusters")]
pub struct ClustersConfig;
