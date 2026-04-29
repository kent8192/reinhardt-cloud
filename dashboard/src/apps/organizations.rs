//! Organizations app — multi-tenant ownership boundaries.
//!
//! Provides `Organization` and `OrganizationMembership` models plus the
//! supporting repositories. The RBAC permission matrix and view-level
//! guard live in [`permissions`] (issue #417). K8s namespace lifecycle
//! is handled by sub-issue #416.

pub mod helpers;
pub mod models;
pub mod permissions;
pub mod roles;
pub mod services;
pub mod urls;

#[cfg(test)]
mod tests;
