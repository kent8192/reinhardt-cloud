//! Deployments application module.
//!
//! Provides endpoints for application deployment management.

use reinhardt::app_config;

pub mod admin;
pub mod models;
pub mod serializers;
pub mod tests;
pub mod urls;
pub mod views;

#[app_config(name = "deployments", label = "deployments")]
pub struct DeploymentsConfig;
