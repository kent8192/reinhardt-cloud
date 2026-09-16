//! Reinhardt Cloud — Kubernetes-native PaaS control plane
//!
//! This is the reinhardt startproject application crate. It re-exports
//! library crates from `crates/` for centralized access and contains
//! Django-style apps (auth, clusters, deployments) in `src/apps/`.
//! Client pages, server-function stubs, and generated form contracts compile
//! on native and WASM; persistence and service implementations are native-only.
//! The browser launcher guards authenticated routes before mounting them.

// Re-export library crates for centralized access.
#[cfg(native)]
pub use reinhardt_cloud_core;
#[cfg(native)]
pub use reinhardt_cloud_k8s;
#[cfg(native)]
pub use reinhardt_cloud_types;

// Application modules — available on both platforms with conditional submodules.
pub mod apps;
// `client` is cross-target so native compilation checks route declarations.
// ClientLauncher constructs the active SPA tree on WASM; native tests create
// explicit ClientRouter instances inside a ReactiveScope.
pub mod client;
pub mod config;
#[cfg(native)]
pub mod server;
pub mod shared;
#[cfg(native)]
pub mod utils;

// Re-export commonly used items
#[cfg(native)]
pub use config::settings::{ProjectSettings, get_settings};
#[cfg(native)]
pub use config::urls::routes;
