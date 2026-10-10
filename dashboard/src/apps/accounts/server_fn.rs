//! Server functions for the accounts application.
//!
//! Server functions are callable from the WASM client. The `#[server_fn]`
//! attribute generates both native and WASM-side bindings, so **do not add
//! `#[cfg(server)]` to the `#[server_fn]` items themselves** — the macro
//! handles target dispatch internally.
//!
//! Server-only imports and helper functions, however, must be gated with
//! `#[cfg(server)]` because they reference types (database connection,
//! session middleware, models, ...) that are not compiled on the WASM
//! target.

pub mod current_viewer;
pub mod sign_out;
pub mod take_sign_in_notice;
