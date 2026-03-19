//! Auth server functions for frontend WASM communication.
//!
//! These functions use the `#[server_fn]` macro to generate both server-side
//! handlers and client-side WASM stubs from a single definition. The macro
//! handles conditional compilation: on the server the original async function
//! runs, while on WASM a POST stub is generated automatically.
//!
//! Access individual functions via their submodules:
//! - `server::login::login`
//! - `server::register::register`
//! - `server::logout::logout`
//! - `server::me::me`

pub mod login;
pub mod logout;
pub mod me;
pub mod register;
