//! Static role x action permission matrix.
//!
//! The matrix is encoded as a single `match (role, action)` rather than
//! a hash map so that the compiler enforces exhaustiveness whenever a
//! new variant is added to [`Action`] or [`MembershipRole`]. Adding a
//! variant without updating the matrix is a compile error.

use crate::apps::organizations::permissions::action::Action;
use crate::apps::organizations::roles::MembershipRole;

/// Returns true if `role` is permitted to perform `action`.
///
/// # Role hierarchy
///
/// - `Owner`: org-level superuser — every action.
/// - `Admin`: every action except destroying the organization itself
///   and changing an existing Owner's role.
/// - `Developer` (corresponds to issue #417's "Member"): create/read/
///   update/delete own resources, read org metadata, no member
///   management.
/// - `Viewer`: read-only across the org.
///
/// # Note on "own vs all"
///
/// The static matrix encodes capability per role; per-instance ownership
/// (e.g. "Developer can update only their own deployment") is enforced
/// downstream by the existing `organization_id` filter on each query and
/// is intentionally outside this function's responsibility.
pub fn allowed(role: MembershipRole, action: Action) -> bool {
	match (role, action) {
		// =============================================================
		// Owner — every action
		// =============================================================
		(MembershipRole::Owner, _) => true,

		// =============================================================
		// Admin
		// =============================================================
		// Admin can do everything except destroy the org or alter Owner
		// roles. `MemberChangeRole` is allowed at the matrix level; the
		// "cannot demote/promote an Owner" rule belongs to the member
		// management view, which inspects the target member's current
		// role before permitting the change.
		(MembershipRole::Admin, Action::OrgDelete) => false,
		(MembershipRole::Admin, _) => true,

		// =============================================================
		// Developer (the issue's "Member")
		// =============================================================
		(MembershipRole::Developer, Action::OrgRead) => true,
		(MembershipRole::Developer, Action::OrgUpdate) => false,
		(MembershipRole::Developer, Action::OrgDelete) => false,

		(MembershipRole::Developer, Action::MemberInvite) => false,
		(MembershipRole::Developer, Action::MemberRemove) => false,
		(MembershipRole::Developer, Action::MemberChangeRole) => false,

		(MembershipRole::Developer, Action::ClusterCreate) => true,
		(MembershipRole::Developer, Action::ClusterRead) => true,
		(MembershipRole::Developer, Action::ClusterUpdate) => true,
		(MembershipRole::Developer, Action::ClusterDelete) => true,

		(MembershipRole::Developer, Action::DeploymentCreate) => true,
		(MembershipRole::Developer, Action::DeploymentRead) => true,
		(MembershipRole::Developer, Action::DeploymentUpdate) => true,
		(MembershipRole::Developer, Action::DeploymentDelete) => true,

		(MembershipRole::Developer, Action::LogsRead) => true,

		// =============================================================
		// Viewer — read-only
		// =============================================================
		(MembershipRole::Viewer, Action::OrgRead) => true,
		(MembershipRole::Viewer, Action::ClusterRead) => true,
		(MembershipRole::Viewer, Action::DeploymentRead) => true,
		(MembershipRole::Viewer, Action::LogsRead) => true,

		(MembershipRole::Viewer, Action::OrgUpdate) => false,
		(MembershipRole::Viewer, Action::OrgDelete) => false,
		(MembershipRole::Viewer, Action::MemberInvite) => false,
		(MembershipRole::Viewer, Action::MemberRemove) => false,
		(MembershipRole::Viewer, Action::MemberChangeRole) => false,
		(MembershipRole::Viewer, Action::ClusterCreate) => false,
		(MembershipRole::Viewer, Action::ClusterUpdate) => false,
		(MembershipRole::Viewer, Action::ClusterDelete) => false,
		(MembershipRole::Viewer, Action::DeploymentCreate) => false,
		(MembershipRole::Viewer, Action::DeploymentUpdate) => false,
		(MembershipRole::Viewer, Action::DeploymentDelete) => false,
	}
}
