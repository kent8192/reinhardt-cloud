//! Cluster ORM model.

use reinhardt::prelude::*;
use serde::{Deserialize, Serialize};

/// Kubernetes cluster registered with the nuages PaaS.
#[derive(Serialize, Deserialize)]
#[model(app_label = "clusters", table_name = "clusters")]
pub struct Cluster {
	/// Primary key (None for auto-increment on insert)
	#[field(primary_key = true)]
	pub id: Option<i64>,

	/// Cluster display name
	#[field(max_length = 255)]
	pub name: String,

	/// Kubernetes API server URL
	#[field(max_length = 1024)]
	pub api_url: String,

	/// Whether the cluster is active and accepting deployments
	#[field(default = true)]
	pub is_active: bool,

	/// Cluster registration timestamp
	#[field(auto_now_add = true)]
	pub created_at: chrono::DateTime<chrono::Utc>,

	/// Last update timestamp
	#[field(auto_now = true)]
	pub updated_at: chrono::DateTime<chrono::Utc>,
}
