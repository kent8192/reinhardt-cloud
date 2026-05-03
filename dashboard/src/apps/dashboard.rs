//! Dashboard application module.
//!
//! Owns the project-level SPA route table — the home/clusters/deployments
//! pages reachable from the top-level navigation. Views are pulled from
//! `crate::client` until each section grows its own per-app implementation.
//!
//! `urls` and `app_config` are native-only because `#[url_patterns]` and
//! `#[app_config]` are gated behind `#[cfg(native)]` in the framework.
//! `views` stays cross-target because it only re-exports cross-target
//! items from `crate::client`. SPA URL resolution from cross-target
//! files goes through `crate::client::url::url_for_spa` until upstream
//! kent8192/reinhardt-web#4119 lifts the wasm gate on per-app typed
//! accessors (tracked in #534).

#[cfg(native)]
use reinhardt::app_config;

#[cfg(native)]
pub mod urls;
pub mod views;

#[cfg(native)]
#[app_config(name = "dashboard", label = "dashboard")]
pub struct DashboardConfig;
