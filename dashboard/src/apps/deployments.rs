//! Deployments application module.
//!
//! Provides deployment management server functions and SPA pages.

#[cfg(native)]
use reinhardt::app_config;

#[cfg(client)]
pub mod client;

#[cfg(server)]
pub mod admin;
#[cfg(server)]
pub mod models;
#[cfg(server)]
pub mod serializers;
pub mod server_fn;
#[cfg(server)]
pub mod server_urls;
#[cfg(server)]
pub mod services;
#[cfg(server)]
pub mod tests;
pub mod urls;

#[cfg(server)]
#[app_config(name = "deployments", label = "deployments")]
pub struct DeploymentsConfig;
