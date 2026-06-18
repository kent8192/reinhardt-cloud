//! Auth application module.
//!
//! Provides cookie-backed authentication server functions and client pages.

#[cfg(native)]
use reinhardt::app_config;

#[cfg(native)]
pub mod admin;
// `client` is intentionally cross-target so page constructors are
// available for `UnifiedRouter::client(...)` reverse URL registration
// in `crate::config::urls::make_router` (kent8192/reinhardt-web#4068).
pub mod client;
#[cfg(native)]
pub mod middleware;
#[cfg(native)]
pub mod models;
#[cfg(native)]
pub mod serializers;
#[cfg(native)]
pub mod server_urls;
// Available on both native and WASM: `#[server_fn]` generates client-side
// POST stubs on WASM while keeping full implementations on native.
pub mod server_fn;
#[cfg(native)]
pub mod services;
#[cfg(native)]
pub mod tests;
pub mod urls;

#[cfg(native)]
#[app_config(name = "auth", label = "auth")]
pub struct AuthConfig;
