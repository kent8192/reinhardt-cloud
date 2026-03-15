//! Deployment ORM model.

use reinhardt::prelude::*;
use serde::{Deserialize, Serialize};

/// Application deployment targeting a specific cluster.
#[derive(Serialize, Deserialize)]
#[model(app_label = "deployments", table_name = "deployments")]
pub struct Deployment {
	/// Primary key (None for auto-increment on insert)
	#[field(primary_key = true)]
	pub id: Option<i64>,

	/// Application name
	#[field(max_length = 255)]
	pub app_name: String,

	/// Foreign key to clusters table
	pub cluster_id: i64,

	/// Deployment lifecycle status (pending, running, failed, succeeded)
	#[field(max_length = 50, default = "pending")]
	pub status: String,

	/// Container image reference
	#[field(max_length = 512)]
	pub image: String,

	/// Deployment creation timestamp
	#[field(auto_now_add = true)]
	pub created_at: chrono::DateTime<chrono::Utc>,

	/// Last update timestamp
	#[field(auto_now = true)]
	pub updated_at: chrono::DateTime<chrono::Utc>,
}
