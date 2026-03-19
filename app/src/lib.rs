//! nuages — Kubernetes-native PaaS control plane
//!
//! This is the reinhardt startproject application crate. It re-exports
//! library crates from `crates/` for centralized access and contains
//! Django-style apps (auth, clusters, deployments) in `src/apps/`.

// Re-export library crates for centralized access (native only).
#[cfg(native)]
pub use nuages_core;
#[cfg(native)]
pub use nuages_k8s;
#[cfg(native)]
pub use nuages_types;

// Application modules
#[cfg(native)]
pub mod apps;
#[cfg(wasm)]
pub mod client;
#[cfg(native)]
pub mod config;
pub mod shared;

// Re-export commonly used items (native only).
#[cfg(native)]
pub use config::settings::get_settings;
#[cfg(native)]
pub use config::urls::routes;
