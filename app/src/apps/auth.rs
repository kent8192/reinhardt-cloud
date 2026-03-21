//! Auth application module.
//!
//! Provides JWT-based authentication endpoints for login and registration.
//! On WASM, only server function stubs and client pages are available.

#[cfg(native)]
use reinhardt::app_config;

#[cfg(native)]
pub mod admin;
#[cfg(wasm)]
pub mod client;
#[cfg(native)]
pub mod models;
#[cfg(native)]
pub mod serializers;
#[cfg(native)]
pub mod server;
#[cfg(native)]
pub mod services;
#[cfg(native)]
pub mod tests;
#[cfg(native)]
pub mod urls;
#[cfg(native)]
pub mod views;

#[cfg(native)]
#[app_config(name = "auth", label = "auth")]
pub struct AuthConfig;
