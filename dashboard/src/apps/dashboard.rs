//! Dashboard application module.
//!
//! Owns the project-level SPA route table — the home/clusters/deployments
//! pages reachable from the top-level navigation. Views are pulled from
//! `crate::client` until each section grows its own per-app implementation.
//!
//! `urls` and `app_config` are native-only because the `#[url_patterns]`
//! / `#[app_config]` macro expansions reference framework re-exports
//! (`reinhardt_apps`, `urls`, `app_config`, `WebSocketRouter`) that are
//! still `#[cfg(native)]` even after kent8192/reinhardt-web#4132 / #4156.
//! `views` stays cross-target because it only re-exports cross-target
//! items from `crate::client`. SPA URL resolution from cross-target
//! files goes through `crate::client::url::url_for_spa` until upstream
//! kent8192/reinhardt-web#4161 lifts those four gates (tracked in #540).

#[cfg(native)]
use reinhardt::app_config;

#[cfg(native)]
pub mod urls;
pub mod views;

#[cfg(native)]
#[app_config(name = "dashboard", label = "dashboard")]
pub struct DashboardConfig;
