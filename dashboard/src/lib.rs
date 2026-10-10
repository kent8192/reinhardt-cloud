//! cloud_control_plane library
//!
//! Library crate of the Control Plane application. Module layout:
//! - `apps`         — application code (each app has server-side routes and client-side pages)
//! - `audit`        — the shared audit-event helper (server only)
//! - `persisted_time` — microsecond-truncated timestamps for persistence
//! - `client`       — WASM-only frontend (booted by its `wasm_bindgen(start)` entry point)
//! - `config`       — project configuration (settings, urls, apps, wasm)
//! - `i18n`         — message catalogs and the page i18n context
//! - `ui`           — design-system components and layouts shared by every app
//! - `server`       — production HTTP server bootstrap used by the server binary

// Server-only re-exports for macro-generated code.
//
// Server-side macros (`#[routes]`, `#[server_fn]`, etc.) reference framework
// crates by their internal paths (`reinhardt_apps`, `reinhardt_core`, ...).
// Re-export them from the crate root so the generated code resolves regardless
// of feature combination.
#[cfg(server)]
pub use reinhardt::core::async_trait;
#[cfg(server)]
pub use reinhardt::reinhardt_apps;
#[cfg(server)]
pub use reinhardt::reinhardt_core;
#[cfg(server)]
pub use reinhardt::reinhardt_di::params;
#[cfg(server)]
pub use reinhardt::reinhardt_http;

// Application modules
pub mod apps;
#[cfg(server)]
pub mod audit;
pub mod config;
pub mod i18n;
pub mod persisted_time;
#[cfg(server)]
pub mod server;
pub mod ui;

// Client-only modules (WASM)
#[cfg(client)]
pub mod client;

// Re-export commonly used items
#[cfg(server)]
pub use config::settings::get_settings;
#[cfg(server)]
pub use config::urls::routes;
