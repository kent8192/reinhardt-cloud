//! Deployment ORM model.

use reinhardt::db::associations::ForeignKeyField;
use reinhardt::prelude::*;
use serde::{Deserialize, Serialize};

use crate::apps::clusters::models::Cluster;
use crate::apps::organizations::models::Organization;

/// Application deployment targeting a specific cluster.
#[model(app_label = "deployments", table_name = "deployments")]
#[derive(Serialize, Deserialize)]
pub struct Deployment {
	/// Primary key (None for auto-increment on insert)
	#[field(primary_key = true)]
	pub id: Option<i64>,

	/// Organization that owns this deployment.
	#[rel(foreign_key, related_name = "deployments")]
	pub organization: ForeignKeyField<Organization>,

	/// Application name
	#[field(max_length = 255)]
	pub app_name: String,

	/// Cluster targeted by this deployment.
	#[rel(foreign_key, related_name = "deployments")]
	pub cluster: ForeignKeyField<Cluster>,

	/// Deployment lifecycle status (pending, running, failed, succeeded)
	#[field(max_length = 50, default = "pending")]
	pub status: String,

	/// Container image reference
	#[field(max_length = 512)]
	pub image: String,

	/// Submitted `ReinhardtApp` manifest YAML for operator-driven deployment.
	#[field(max_length = 65535)]
	pub reinhardt_app_yaml: Option<String>,

	/// Deployment creation timestamp
	#[field(auto_now_add = true)]
	pub created_at: chrono::DateTime<chrono::Utc>,

	/// Last update timestamp
	#[field(auto_now = true)]
	pub updated_at: chrono::DateTime<chrono::Utc>,
}
