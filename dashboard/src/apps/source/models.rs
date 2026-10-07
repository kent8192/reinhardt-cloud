//! Persistence owned by the source App.

use crate::apps::organization::models::Organization;
use chrono::{DateTime, Utc};
use reinhardt::db::associations::ForeignKeyField;
use reinhardt::model;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[model(
	app_label = "source",
	table_name = "cloud_source_repositoryconnection",
	info = false
)]
#[derive(Clone, Serialize, Deserialize)]
pub struct RepositoryConnection {
	#[field(primary_key = true)]
	pub id: Uuid,
	#[rel(foreign_key, on_delete = Cascade)]
	pub organization: ForeignKeyField<Organization>,
	#[field]
	pub github_installation_id: i64, // nosemgrep: reinhardt-no-scalar-fk-id -- External GitHub installation identity.
	#[field(auto_now_add = true)]
	pub created_at: DateTime<Utc>,
}
