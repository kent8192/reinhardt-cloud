//! Membership persistence for the organization App.

use super::Organization;
use crate::apps::identity::models::UserAccount;
use reinhardt::db::associations::ForeignKeyField;
use reinhardt::model;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
