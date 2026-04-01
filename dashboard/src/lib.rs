//! Reinhardt Cloud — Kubernetes-native PaaS control plane
//!
//! This is the reinhardt startproject application crate. It re-exports
//! library crates from `crates/` for centralized access and contains
//! Django-style apps (auth, clusters, deployments) in `src/apps/`.

// Re-export library crates for centralized access.
pub use reinhardt_cloud_core;
pub use reinhardt_cloud_k8s;
pub use reinhardt_cloud_types;

// Application modules
pub mod apps;
pub mod config;

// Re-export commonly used items
pub use config::settings::get_settings;
pub use config::urls::routes;
