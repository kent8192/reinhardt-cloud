//! Organization membership ORM model: user x organization with a role.

use reinhardt::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Membership association between an `auth_users` row and an `organizations`
/// row, with a stored role string. Role values are constrained at the DB
/// layer by `CHECK (role IN ('owner','admin','developer','viewer'))`; the
/// application layer parses via `MembershipRole::from_db_str`.
#[derive(Serialize, Deserialize)]
#[model(app_label = "organizations", table_name = "organization_memberships")]
pub struct OrganizationMembership {
	/// Primary key (None for auto-increment on insert).
	#[field(primary_key = true)]
	pub id: Option<i64>,

	/// FK to `organizations.id`.
	pub organization_id: i64,

	/// FK to `auth_users.id` (Uuid PK in reinhardt-web).
	pub user_id: Uuid,

	/// Lowercase role string. Validated by Rust enum
	/// `crate::apps::organizations::roles::MembershipRole` and constrained
	/// at the DB layer by a CHECK constraint.
	#[field(max_length = 20)]
	pub role: String,

	/// Timestamp the membership was granted.
	#[field(auto_now_add = true)]
	pub created_at: chrono::DateTime<chrono::Utc>,
}
