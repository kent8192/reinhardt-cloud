//! Exhaustive permission matrix tests for issue #417.
//!
//! Verifies `allowed(role, action)` for every (role, action) pair so a
//! regression in the matrix is caught immediately. This file is the
//! canonical specification of the role-to-action policy — adding a new
//! `Action` variant requires adding rows here.

use rstest::rstest;

use crate::apps::organizations::permissions::{Action, allowed};
use crate::apps::organizations::roles::MembershipRole;

// ============================================================================
// Owner — every action allowed
// ============================================================================

#[rstest]
#[case(Action::OrgRead)]
#[case(Action::OrgUpdate)]
#[case(Action::OrgDelete)]
#[case(Action::MemberInvite)]
#[case(Action::MemberRemove)]
#[case(Action::MemberChangeRole)]
#[case(Action::ClusterCreate)]
#[case(Action::ClusterRead)]
#[case(Action::ClusterUpdate)]
#[case(Action::ClusterDelete)]
#[case(Action::DeploymentCreate)]
#[case(Action::DeploymentRead)]
#[case(Action::DeploymentUpdate)]
#[case(Action::DeploymentDelete)]
#[case(Action::LogsRead)]
fn owner_can_perform_every_action(#[case] action: Action) {
	// Arrange / Act
	let actual = allowed(MembershipRole::Owner, action);

	// Assert
	assert!(
		actual,
		"Owner must be permitted to perform {action:?}; matrix returned false"
	);
}

// ============================================================================
// Admin — every action except OrgDelete
// ============================================================================

#[rstest]
#[case(Action::OrgRead, true)]
#[case(Action::OrgUpdate, true)]
#[case(Action::OrgDelete, false)] // Only Owner can delete the org
#[case(Action::MemberInvite, true)]
#[case(Action::MemberRemove, true)]
#[case(Action::MemberChangeRole, true)]
#[case(Action::ClusterCreate, true)]
#[case(Action::ClusterRead, true)]
#[case(Action::ClusterUpdate, true)]
#[case(Action::ClusterDelete, true)]
#[case(Action::DeploymentCreate, true)]
#[case(Action::DeploymentRead, true)]
#[case(Action::DeploymentUpdate, true)]
#[case(Action::DeploymentDelete, true)]
#[case(Action::LogsRead, true)]
fn admin_matrix(#[case] action: Action, #[case] expected: bool) {
	// Arrange / Act
	let actual = allowed(MembershipRole::Admin, action);

	// Assert
	assert_eq!(
		actual, expected,
		"Admin x {action:?} should be {expected}, got {actual}"
	);
}

// ============================================================================
// Developer (the issue's "Member")
// ============================================================================

#[rstest]
#[case(Action::OrgRead, true)]
#[case(Action::OrgUpdate, false)]
#[case(Action::OrgDelete, false)]
#[case(Action::MemberInvite, false)]
#[case(Action::MemberRemove, false)]
#[case(Action::MemberChangeRole, false)]
#[case(Action::ClusterCreate, true)]
#[case(Action::ClusterRead, true)]
#[case(Action::ClusterUpdate, true)]
#[case(Action::ClusterDelete, true)]
#[case(Action::DeploymentCreate, true)]
#[case(Action::DeploymentRead, true)]
#[case(Action::DeploymentUpdate, true)]
#[case(Action::DeploymentDelete, true)]
#[case(Action::LogsRead, true)]
fn developer_matrix(#[case] action: Action, #[case] expected: bool) {
	// Arrange / Act
	let actual = allowed(MembershipRole::Developer, action);

	// Assert
	assert_eq!(
		actual, expected,
		"Developer x {action:?} should be {expected}, got {actual}"
	);
}

// ============================================================================
// Viewer — read-only across the org
// ============================================================================

#[rstest]
#[case(Action::OrgRead, true)]
#[case(Action::OrgUpdate, false)]
#[case(Action::OrgDelete, false)]
#[case(Action::MemberInvite, false)]
#[case(Action::MemberRemove, false)]
#[case(Action::MemberChangeRole, false)]
#[case(Action::ClusterCreate, false)]
#[case(Action::ClusterRead, true)]
#[case(Action::ClusterUpdate, false)]
#[case(Action::ClusterDelete, false)]
#[case(Action::DeploymentCreate, false)]
#[case(Action::DeploymentRead, true)]
#[case(Action::DeploymentUpdate, false)]
#[case(Action::DeploymentDelete, false)]
#[case(Action::LogsRead, true)]
fn viewer_matrix(#[case] action: Action, #[case] expected: bool) {
	// Arrange / Act
	let actual = allowed(MembershipRole::Viewer, action);

	// Assert
	assert_eq!(
		actual, expected,
		"Viewer x {action:?} should be {expected}, got {actual}"
	);
}

// ============================================================================
// Hierarchy invariants
// ============================================================================

/// Sanity check: anything a Viewer can do, Developer/Admin/Owner can also do.
#[rstest]
#[case(Action::OrgRead)]
#[case(Action::ClusterRead)]
#[case(Action::DeploymentRead)]
#[case(Action::LogsRead)]
fn higher_roles_inherit_viewer_permissions(#[case] action: Action) {
	// Arrange
	let viewer_allowed = allowed(MembershipRole::Viewer, action);

	// Act / Assert
	assert!(viewer_allowed, "precondition: Viewer can perform {action:?}");
	assert!(
		allowed(MembershipRole::Developer, action),
		"Developer must inherit Viewer permission for {action:?}"
	);
	assert!(
		allowed(MembershipRole::Admin, action),
		"Admin must inherit Viewer permission for {action:?}"
	);
	assert!(
		allowed(MembershipRole::Owner, action),
		"Owner must inherit Viewer permission for {action:?}"
	);
}

/// Sanity check: only Owner is permitted to delete the organization.
#[rstest]
fn only_owner_can_delete_org() {
	// Arrange / Act / Assert
	assert!(allowed(MembershipRole::Owner, Action::OrgDelete));
	assert!(!allowed(MembershipRole::Admin, Action::OrgDelete));
	assert!(!allowed(MembershipRole::Developer, Action::OrgDelete));
	assert!(!allowed(MembershipRole::Viewer, Action::OrgDelete));
}
