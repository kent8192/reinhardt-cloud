//! Application registry for Reinhardt Cloud
//!
//! Django-style apps providing auth, cluster management, and deployment APIs.
//! On WASM, only the auth module is available (for server function stubs and
//! client pages). Other app modules are server-only.

pub mod auth;
// Each app's parent module is cross-target so its `urls` submodule is
// reachable on wasm. Per-app internals (admin, models, serializers,
// services, tests, views) remain native-only via cfg gates inside each
// `<app>.rs`.
pub mod clusters;
pub mod dashboard;
pub mod deployments;
pub mod health;
pub mod organizations;
#[cfg(native)]
pub mod validators;
