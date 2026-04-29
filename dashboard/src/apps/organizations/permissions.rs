//! Role-based access control (RBAC) types and helpers.
//!
//! Provides the [`Action`] enum (the closed set of permission-checked
//! operations), the [`allowed`] function (a static role-to-action matrix),
//! and the [`require_permission`] view helper used by handlers to enforce
//! permission at the request boundary. Together these replace the ad-hoc
//! `if obj.user_id != current_user_id { return Err(404) }` checks that
//! existed prior to issue #417.
//!
//! # Design
//!
//! - The Action set is closed and small (~15 actions, 4 roles), so a static
//!   `match` (rather than a runtime policy engine such as Casbin) keeps
//!   the surface auditable and zero-cost at runtime.
//! - Membership lookup is the cross-org boundary: a user without a
//!   membership row in the requested Organization is rejected with 403,
//!   and never falls through to per-resource checks.
//! - 403 vs 404: the guard returns 403 (`AppError::Authorization`) when
//!   the user is authenticated but lacks permission, and lets the view
//!   surface 404 separately when a resource is missing within an org
//!   the user *does* belong to. This avoids leaking org existence.
//!
//! # Extending
//!
//! When adding a new action:
//!
//! 1. Add a variant to [`Action`].
//! 2. Add a row to [`allowed`] covering all four roles.
//! 3. Add the new `(role, action)` pair to the unit test matrix in
//!    `tests/unit/test_permissions.rs`.

pub mod action;
pub mod guard;
pub mod table;

pub use action::Action;
pub use guard::{require_permission, resolve_membership_role};
pub use table::allowed;
