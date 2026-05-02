//! Application registry for Reinhardt Cloud
//!
//! Django-style apps providing auth, cluster management, and deployment APIs.
//! On WASM, only the auth module is available (for server function stubs and
//! client pages). Other app modules are server-only.

pub mod auth;
#[cfg(native)]
pub mod clusters;
pub mod dashboard;
// `deployments` is server-only at the module level, but it owns a `client`
// submodule containing WASM UI components consumed by `crate::client::ws`.
// The cfg gating that excludes server-only sources lives inside
// `apps/deployments.rs`, so the parent module declaration is unconditional.
pub mod deployments;
#[cfg(native)]
pub mod health;
#[cfg(native)]
pub mod organizations;
#[cfg(native)]
pub mod validators;
