//! Organizations app — multi-tenant ownership boundaries.
//!
//! Provides `Organization` and `OrganizationMembership` models plus the
//! supporting repositories. K8s namespace lifecycle and the `Guard<P>`
//! middleware are handled by sub-issues #416 and #417 respectively.

pub mod helpers;
pub mod models;
pub mod roles;
pub mod services;
pub mod urls;

#[cfg(test)]
mod tests;
