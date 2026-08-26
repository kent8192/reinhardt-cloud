//! GitHub App integration module.
//!
//! Owns GitHub App installation metadata and repository listing for source
//! deployments. OAuth login identity stays in the `auth` app.

#[cfg(native)]
use reinhardt::app_config;

#[cfg(client)]
pub mod client;
#[cfg(server)]
pub mod models;
pub mod server_fn;
#[cfg(server)]
pub mod server_urls;
#[cfg(server)]
pub mod services;
#[cfg(server)]
pub mod tests;
pub mod urls;

#[cfg(server)]
#[app_config(name = "github", label = "github")]
pub struct GitHubConfig;
