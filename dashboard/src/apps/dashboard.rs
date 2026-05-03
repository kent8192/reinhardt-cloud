//! Dashboard application module.
//!
//! Owns the project-level SPA route table — the home/clusters/deployments
//! pages reachable from the top-level navigation. Views are pulled from
//! `crate::client` until each section grows its own per-app implementation.
//!
//! `urls` and `app_config` are native-only because `#[url_patterns]` and
//! `#[app_config]` are gated behind `#[cfg(native)]` in the framework.
//! `views` stays cross-target because it only re-exports cross-target
//! items from `crate::client`. The typed SPA accessor
//! `urls.client().dashboard().<route>()` works on WASM via the global
//! `ClientUrlReverser` populated at runtime, independent of this
//! native-only `url_patterns()` declaration.

#[cfg(native)]
use reinhardt::app_config;

#[cfg(native)]
pub mod urls;
pub mod views;

#[cfg(native)]
#[app_config(name = "dashboard", label = "dashboard")]
pub struct DashboardConfig;
