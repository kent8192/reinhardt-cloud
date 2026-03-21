//! nuages — Kubernetes-native PaaS control plane
//!
//! This is the reinhardt startproject application crate. It re-exports
//! library crates from `crates/` for centralized access and contains
//! Django-style apps (auth, clusters, deployments) in `src/apps/`.
//! On WASM, only the auth module is available (for server function stubs
//! and client pages). Other app modules are server-only.

// Re-export library crates for centralized access.
#[cfg(native)]
pub use reinhardt_cloud_core;
#[cfg(native)]
pub use reinhardt_cloud_k8s;
#[cfg(native)]
pub use reinhardt_cloud_types;

// Application modules — available on both platforms with conditional submodules.
pub mod apps;
#[cfg(any(wasm, test))]
pub mod client;
#[cfg(native)]
pub mod config;
pub mod shared;

// Re-export commonly used items
#[cfg(native)]
pub use config::settings::get_settings;
#[cfg(native)]
pub use config::urls::routes;
