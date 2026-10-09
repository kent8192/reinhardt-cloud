//! cloud_control_plane library
//!
//! Top-level crate for cloud_control_plane. The module layout follows the
//! Reinhardt basics tutorial:
//! - `apps`         — application code (each app has server-side routes and client-side pages)
//! - `client`       — WASM-only frontend (mounted by `bin/manage.rs`)
//! - `config`       — project configuration (settings, urls, apps, wasm)
//! - `server`       — production HTTP server bootstrap used by the server binary

// Server-only re-exports for macro-generated code.
//
// Server-side macros (`#[routes]`, `#[server_fn]`, etc.) reference framework
// crates by their internal paths (`reinhardt_apps`, `reinhardt_core`, ...).
// Re-export them under `crate::*` so the generated code resolves regardless
// of feature combination.
#[cfg(server)]
mod server_only {
	pub use reinhardt::core::async_trait;
	pub use reinhardt::reinhardt_apps;
	pub use reinhardt::reinhardt_core;
	pub use reinhardt::reinhardt_di::params;
	pub use reinhardt::reinhardt_http;
}
#[cfg(server)]
pub use server_only::*;

// Application modules
pub mod apps;
pub mod config;
#[cfg(server)]
pub mod server;

// Client-only modules (WASM)
#[cfg(client)]
pub mod client;

// Re-export commonly used items
#[cfg(server)]
pub use config::settings::get_settings;
#[cfg(server)]
pub use config::urls::routes;
