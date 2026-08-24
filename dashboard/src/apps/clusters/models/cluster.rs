//! Cluster ORM model.

use reinhardt::db::associations::ForeignKeyField;
use reinhardt::prelude::*;
use serde::{Deserialize, Serialize};

use crate::apps::organizations::models::Organization;

/// Kubernetes cluster registered with the Reinhardt Cloud PaaS.
///
/// Composite UNIQUE constraint on `(organization_id, name)` prevents
/// duplicate cluster names within the same organization. Cross-organization
/// name reuse is intentionally allowed so that distinct tenants can each
/// own a `prod` (or other common name) without colliding.
#[model(
	app_label = "clusters",
	table_name = "clusters",
	unique_together = ("organization_id", "name"),
	form = true
)]
#[derive(Clone, Serialize, Deserialize)]
pub struct Cluster {
	/// Primary key (None for auto-increment on insert)
	#[field(primary_key = true)]
	pub id: Option<i64>,

	/// Organization that owns this cluster.
	// Workaround for kent8192/reinhardt-web#6148 (tracked in
	// kent8192/reinhardt-cloud#872). Remove this workaround when a required
	// generated relation ID can be non-editable without referencing an undeclared
	// deferred_field in FormModel::build_from_payload.
	//
	// Ideal implementation (without workaround):
	//   #[field(editable = false)]
	#[rel(foreign_key, related_name = "clusters", on_delete = Cascade)]
	pub organization: ForeignKeyField<Organization>,

	/// Cluster display name
	#[field(min_length = 1, max_length = 63)]
	pub name: String,

	/// Kubernetes API server URL
	#[field(url = true, max_length = 2048)]
	pub api_url: String,

	/// Whether the cluster is active and accepting deployments
	#[field(default = true, editable = false)]
	pub is_active: bool,

	/// Argon2id hash of the cluster agent JWT token.
	///
	/// The plaintext token is returned exactly once on cluster creation or
	/// rotation — only this hash is persisted. `None` indicates that a
	/// token has not yet been issued (legacy clusters pre-dating token
	/// issuance).
	#[field(max_length = 255, editable = false)]
	pub token_hash: Option<String>,

	/// Timestamp of the most recent token rotation. `None` when no token
	/// has ever been issued.
	#[field(editable = false)]
	pub token_last_rotated_at: Option<chrono::DateTime<chrono::Utc>>,

	/// Cluster registration timestamp
	#[field(auto_now_add = true)]
	pub created_at: chrono::DateTime<chrono::Utc>,

	/// Last update timestamp
	#[field(auto_now = true)]
	pub updated_at: chrono::DateTime<chrono::Utc>,
}
