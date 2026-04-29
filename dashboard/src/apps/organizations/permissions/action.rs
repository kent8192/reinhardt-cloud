//! `Action` enum — the closed set of permission-checked operations.

/// Permission-checked operation categories enforced by the RBAC layer.
///
/// Each variant maps to a logical capability rather than a specific HTTP
/// endpoint, so multiple endpoints (e.g. `GET /clusters/`,
/// `GET /clusters/{id}/`) can share the same `ClusterRead` action.
///
/// The set is intentionally small — see
/// [`crate::apps::organizations::permissions::table::allowed`] for the
/// role-to-action matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
	// Organization-level actions
	/// Read organization metadata (name, slug, settings).
	OrgRead,
	/// Update organization metadata.
	OrgUpdate,
	/// Delete an organization.
	OrgDelete,

	// Membership-level actions
	/// Invite a new member to the organization.
	MemberInvite,
	/// Remove a member from the organization.
	MemberRemove,
	/// Change an existing member's role.
	MemberChangeRole,

	// Cluster actions
	/// Create a new cluster.
	ClusterCreate,
	/// Read cluster metadata.
	ClusterRead,
	/// Update cluster fields.
	ClusterUpdate,
	/// Delete a cluster.
	ClusterDelete,

	// Deployment actions
	/// Create a deployment.
	DeploymentCreate,
	/// Read deployment metadata or status.
	DeploymentRead,
	/// Update deployment configuration.
	DeploymentUpdate,
	/// Delete a deployment.
	DeploymentDelete,

	// Logs
	/// Read deployment logs.
	LogsRead,
}
