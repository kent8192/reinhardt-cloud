//! EnvironmentGrant persistence for the organization App.

use super::Organization;
use crate::apps::identity::models::UserAccount;
use reinhardt::db::associations::ForeignKeyField;
use reinhardt::model;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
