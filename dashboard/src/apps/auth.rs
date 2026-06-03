//! Auth application module.
//!
//! Provides JWT-based authentication endpoints for login and registration.
//! On WASM, only server function stubs and client pages are available.

#[cfg(native)]
use reinhardt::app_config;

#[cfg(native)]
pub mod admin;
// `client` is intentionally cross-target so page constructors are
// available for `UnifiedRouter::client(...)` reverse URL registration
// in `crate::config::urls::make_router` (kent8192/reinhardt-web#4068).
pub mod client;
#[cfg(native)]
pub mod models;
#[cfg(native)]
pub mod serializers;
// Available on both native and WASM: #[server_fn] generates client-side
// POST stubs on WASM while keeping full implementations on native.
pub mod server;
#[cfg(native)]
pub mod services;
#[cfg(native)]
pub mod tests;
// `urls` is cross-target so the `#[routes]` macro emits the typed
// accessor reachable from wasm SPA call sites. The closure body's
// `views::*` references are cfg-gated inside `urls.rs`.
pub mod urls;
#[cfg(native)]
pub mod views;

#[cfg(native)]
#[app_config(name = "auth", label = "auth")]
pub struct AuthConfig;
