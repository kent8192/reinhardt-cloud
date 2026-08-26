//! Clusters application module.
//!
//! Provides cluster management server functions and SPA pages.

#[cfg(server)]
use reinhardt::app_config;

#[cfg(server)]
pub mod admin;
#[cfg(client)]
pub mod client;
pub mod model_form;
#[cfg(server)]
pub mod models;
#[cfg(server)]
pub mod serializers;
pub mod server_fn;
#[cfg(server)]
pub mod services;
#[cfg(server)]
pub mod tests;
pub mod urls;

#[cfg(server)]
#[app_config(name = "clusters", label = "clusters")]
pub struct ClustersConfig;
