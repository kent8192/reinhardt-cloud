//! Persistence owned by the project App.

use crate::apps::organization::models::Organization;
use chrono::{DateTime, Utc};
use reinhardt::db::associations::ForeignKeyField;
use reinhardt::model;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[model(
	app_label = "project",
	table_name = "cloud_project_project",
	info = false
)]
#[derive(Clone, Serialize, Deserialize)]
pub struct Project {
	#[field(primary_key = true)]
	pub id: Uuid,
	#[rel(foreign_key, on_delete = Restrict)]
	pub organization: ForeignKeyField<Organization>,
	#[field(max_length = 200)]
	pub display_name: String,
	#[field(max_length = 2048)]
	pub repository_url: String,
	#[field(auto_now_add = true)]
	pub created_at: DateTime<Utc>,
}

#[model(app_label = "project", table_name = "cloud_environment", info = false)]
#[derive(Clone, Serialize, Deserialize)]
pub struct Environment {
	#[field(primary_key = true)]
	pub id: Uuid,
	#[rel(foreign_key, on_delete = Restrict)]
	pub organization: ForeignKeyField<Organization>,
	#[rel(foreign_key, on_delete = Restrict)]
	pub project: ForeignKeyField<Project>,
	#[field(max_length = 32)]
	pub kind: String,
	#[field(default = 0)]
	pub version: i64,
	#[field(field_type = "text", default = "{}")]
	pub desired_runtime: String,
}
