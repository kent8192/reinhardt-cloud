//! nuages — Kubernetes-native PaaS control plane
//!
//! This is the reinhardt startproject application crate. It re-exports
//! library crates from `crates/` for centralized access and contains
//! Django-style apps (auth, clusters, deployments) in `src/apps/`.

// Re-export library crates for centralized access.
pub use nuages_core;
pub use nuages_k8s;
pub use nuages_types;

// Application modules
pub mod apps;
pub mod config;
pub mod migrations;

// Re-export commonly used items
pub use config::settings::get_settings;
pub use config::urls::routes;
pub use migrations::NuagesMigrations;
