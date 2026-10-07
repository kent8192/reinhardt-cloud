//! Persistence owned by the organization App.

use crate::apps::identity::models::UserAccount;
use chrono::{DateTime, Utc};
use reinhardt::db::associations::ForeignKeyField;
use reinhardt::model;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[model(
	app_label = "organization",
	table_name = "cloud_organization_organization",
	info = false
)]
#[derive(Clone, Serialize, Deserialize)]
pub struct Organization {
	#[field(primary_key = true)]
	pub id: Uuid,
	#[field(max_length = 200)]
	pub display_name: String,
	#[field(auto_now_add = true)]
	pub created_at: DateTime<Utc>,
}

#[model(app_label = "organization", table_name = "cloud_membership", info = false, unique_together = ("organization_id", "user_id"))]
#[derive(Clone, Serialize, Deserialize)]
pub struct Membership {
	#[field(primary_key = true)]
	pub id: Uuid,
	#[rel(foreign_key, on_delete = Cascade)]
	pub organization: ForeignKeyField<Organization>,
	#[rel(foreign_key, on_delete = Cascade)]
	pub user: ForeignKeyField<UserAccount>,
	#[field(max_length = 32)]
	pub role: String,
}

/// A production execution grant remains subject to current organization membership.
#[model(app_label = "organization", table_name = "cloud_environment_grant", info = false, unique_together = ("organization_id", "user_id", "environment_id"))]
#[derive(Clone, Serialize, Deserialize)]
pub struct EnvironmentGrant {
	#[field(primary_key = true)]
	pub id: Uuid,
	#[rel(foreign_key, on_delete = Cascade)]
	pub organization: ForeignKeyField<Organization>,
	#[rel(foreign_key, on_delete = Cascade)]
	pub user: ForeignKeyField<UserAccount>,
	#[rel(foreign_key, on_delete = Cascade)]
	pub environment: ForeignKeyField<crate::apps::project::models::Environment>,
}
