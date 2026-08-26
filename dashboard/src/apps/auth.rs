//! Auth application module.
//!
//! Provides cookie-backed authentication server functions and client pages.

#[cfg(server)]
use reinhardt::app_config;

#[cfg(server)]
pub mod admin;
#[cfg(client)]
pub mod client;
#[cfg(server)]
pub mod middleware;
#[cfg(server)]
pub mod models;
pub mod serializers;
#[cfg(server)]
pub mod server_urls;
// Available on both native and WASM: `#[server_fn]` generates client-side
// POST stubs on WASM while keeping full implementations on native.
pub mod server_fn;
#[cfg(server)]
pub mod services;
#[cfg(server)]
pub mod tests;
pub mod urls;

#[cfg(server)]
#[app_config(name = "auth", label = "auth")]
pub struct AuthConfig;
