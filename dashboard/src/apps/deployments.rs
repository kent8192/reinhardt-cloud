//! Deployments application module.
//!
//! Provides deployment management server functions and SPA pages.

#[cfg(native)]
use reinhardt::app_config;

pub mod client;

#[cfg(native)]
pub mod admin;
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
#[app_config(name = "deployments", label = "deployments")]
pub struct DeploymentsConfig;
