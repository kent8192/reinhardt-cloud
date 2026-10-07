//! Project persistence for the project App.

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
