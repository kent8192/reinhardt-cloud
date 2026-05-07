//! Clusters application module.
//!
//! Provides endpoints for Kubernetes cluster management. The `urls`
//! submodule is cross-target so the typed SPA accessor reaches it on
//! wasm; everything else is native-only.

#[cfg(native)]
use reinhardt::app_config;

#[cfg(native)]
pub mod admin;
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
