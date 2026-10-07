//! AuditRecord persistence for the observability App.

use chrono::{DateTime, Utc};
use reinhardt::model;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[model(
	app_label = "observability",
	table_name = "cloud_observability_auditrecord",
	info = false
)]
#[derive(Clone, Serialize, Deserialize)]
pub struct AuditRecord {
	#[field(primary_key = true)]
	pub id: Uuid,
	#[field]
	pub organization_id: Uuid, // nosemgrep: reinhardt-no-scalar-fk-id -- Immutable audit identity retained after source records are deleted.
	#[field]
	pub actor_id: Uuid, // nosemgrep: reinhardt-no-scalar-fk-id -- Immutable audit identity retained after source records are deleted.
	#[field]
	pub target_id: Uuid, // nosemgrep: reinhardt-no-scalar-fk-id -- Immutable audit identity retained after source records are deleted.
	#[field(max_length = 100)]
	pub action: String,
	#[field(auto_now_add = true)]
	pub created_at: DateTime<Utc>,
}
