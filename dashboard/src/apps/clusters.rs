//! Clusters application module.
//!
//! Provides endpoints for Kubernetes cluster management. The `client` and
//! `urls` submodules are cross-target so the typed SPA accessor and SPA
//! page constructors reach the wasm bundle; everything else is
//! native-only.

#[cfg(native)]
use reinhardt::app_config;

#[cfg(native)]
pub mod admin;
// `client` is intentionally cross-target so page constructors are
// available for `UnifiedRouter::client(...)` reverse URL registration in
// `crate::config::urls::make_router` (kent8192/reinhardt-web#4068).
pub mod client;
#[cfg(native)]
pub mod models;
#[cfg(native)]
pub mod serializers;
#[cfg(native)]
pub mod services;
#[cfg(native)]
pub mod tests;
pub mod urls;
#[cfg(native)]
pub mod views;

#[cfg(native)]
#[app_config(name = "clusters", label = "clusters")]
pub struct ClustersConfig;
