//! Organizations app — multi-tenant ownership boundaries.
//!
//! Provides `Organization` and `OrganizationMembership` models plus the
//! supporting repositories. The RBAC permission matrix and view-level
//! guard live in [`permissions`] (issue #417). K8s namespace lifecycle
//! is handled by sub-issue #416. The `urls` submodule is cross-target
//! so the typed SPA accessor reaches it on wasm.

#[cfg(server)]
pub mod helpers;
#[cfg(server)]
pub mod models;
#[cfg(server)]
pub mod permissions;
#[cfg(server)]
pub mod roles;
#[cfg(server)]
pub mod services;
pub mod urls;

#[cfg(all(test, server))]
mod tests;
