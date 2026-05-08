//! Dashboard application module.
//!
//! Owns the project-level SPA `home` route. The dashboard shell layout
//! (rendered for `/`) lives under `client/layout.rs`; per-section pages
//! (`clusters:list`, `deployments:list`) belong to their respective
//! apps.

use reinhardt::app_config;

// `client` is intentionally cross-target so the layout constructor is
// reachable from the wasm SPA call sites and from the
// `UnifiedRouter::client(...)` registration in `urls.rs`.
pub mod client;
pub mod urls;

#[app_config(name = "dashboard", label = "dashboard")]
pub struct DashboardConfig;
