//! Application registry for nuages
//!
//! Django-style apps providing auth, cluster management, and deployment APIs.
//! On WASM, only the auth module is available (for server function stubs and
//! client pages). Other app modules are server-only.

pub mod auth;
#[cfg(native)]
pub mod clusters;
#[cfg(native)]
pub mod deployments;
#[cfg(native)]
pub mod validators;
