//! GitHub App integration module.
//!
//! Owns GitHub App installation metadata and repository listing for source
//! deployments. OAuth login identity stays in the `auth` app.

#[cfg(native)]
use reinhardt::app_config;

pub mod client;
#[cfg(native)]
pub mod models;
pub mod server_fn;
#[cfg(native)]
pub mod server_urls;
#[cfg(native)]
pub mod services;
#[cfg(native)]
pub mod tests;
pub mod urls;

#[cfg(native)]
#[app_config(name = "github", label = "github")]
pub struct GitHubConfig;
