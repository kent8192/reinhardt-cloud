//! Organization membership ORM model: user x organization with a role.

use reinhardt::db::associations::ForeignKeyField;
use reinhardt::prelude::*;
use serde::{Deserialize, Serialize};

use crate::apps::auth::models::User;

use super::Organization;

/// Membership association between an `auth_users` row and an `organizations`
/// row, with a stored role string. Role values are constrained at the DB
/// layer by `CHECK (role IN ('owner','admin','developer','viewer'))`; the
/// application layer parses via `MembershipRole::from_db_str`.
#[model(app_label = "organizations", table_name = "organization_memberships")]
#[derive(Serialize, Deserialize)]
pub struct OrganizationMembership {
	/// Primary key (None for auto-increment on insert).
	#[field(primary_key = true)]
	pub id: Option<i64>,

	/// Organization that owns this membership.
	#[rel(foreign_key, related_name = "memberships")]
	pub organization: ForeignKeyField<Organization>,

	/// User granted membership in the organization.
	#[rel(foreign_key, related_name = "organization_memberships")]
	pub user: ForeignKeyField<User>,

	/// Lowercase role string. Validated by Rust enum
	/// `crate::apps::organizations::roles::MembershipRole` and constrained
	/// at the DB layer by a CHECK constraint.
	#[field(max_length = 20)]
	pub role: String,

	/// Timestamp the membership was granted.
	#[field(auto_now_add = true)]
	pub created_at: chrono::DateTime<chrono::Utc>,
}
