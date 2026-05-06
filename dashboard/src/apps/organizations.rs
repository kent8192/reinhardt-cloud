//! Organizations app — multi-tenant ownership boundaries.
//!
//! Provides `Organization` and `OrganizationMembership` models plus the
//! supporting repositories. The RBAC permission matrix and view-level
//! guard live in [`permissions`] (issue #417). K8s namespace lifecycle
//! is handled by sub-issue #416. The `urls` submodule is cross-target
//! so the typed SPA accessor reaches it on wasm.

#[cfg(native)]
pub mod helpers;
#[cfg(native)]
pub mod models;
#[cfg(native)]
pub mod permissions;
#[cfg(native)]
pub mod roles;
#[cfg(native)]
pub mod services;
pub mod urls;

#[cfg(all(test, native))]
mod tests;
