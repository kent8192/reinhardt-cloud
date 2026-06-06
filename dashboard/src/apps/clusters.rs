//! Clusters application module.
//!
//! Provides cluster management server functions and SPA pages.

#[cfg(native)]
use reinhardt::app_config;

#[cfg(native)]
pub mod admin;
pub mod client;
#[cfg(native)]
pub mod models;
#[cfg(native)]
pub mod serializers;
pub mod server_fn;
#[cfg(native)]
pub mod services;
#[cfg(native)]
pub mod tests;
pub mod urls;

#[cfg(native)]
#[app_config(name = "clusters", label = "clusters")]
pub struct ClustersConfig;
