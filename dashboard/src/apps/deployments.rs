//! Deployments application module.
//!
//! Provides endpoints for application deployment management on the server,
//! and a `client` submodule containing the WASM UI components for the
//! real-time log viewer and cluster health panel.

#[cfg(native)]
use reinhardt::app_config;

// The `client` submodule is consumed from the WASM build path and is
// therefore unconditional. Its own internals are wasm-gated as needed.
pub mod client;

#[cfg(native)]
pub mod admin;
#[cfg(native)]
pub mod models;
#[cfg(native)]
pub mod serializers;
pub mod server;
#[cfg(native)]
pub mod tests;
// `urls` is cross-target so the `#[routes]` macro emits the typed
// accessor reachable from wasm SPA call sites. The closure body's
// `views::*` references are cfg-gated inside `urls.rs`.
pub mod urls;
#[cfg(native)]
pub mod views;

#[cfg(native)]
#[app_config(name = "deployments", label = "deployments")]
pub struct DeploymentsConfig;
