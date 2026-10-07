//! Cluster persistence for the cluster App.

use chrono::{DateTime, Utc};
use reinhardt::model;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[model(
	app_label = "cluster",
	table_name = "cloud_cluster_cluster",
	info = false
)]
#[derive(Clone, Serialize, Deserialize)]
pub struct Cluster {
	#[field(primary_key = true)]
	pub id: Uuid,
	#[field(max_length = 200)]
	pub display_name: String,
	#[field(default = false)]
	pub registered: bool,
	#[field(default = false)]
	pub rootless_builds: bool,
	#[field(default = false)]
	pub ingress: bool,
	#[field(default = false)]
	pub dns: bool,
	#[field(default = false)]
	pub namespace_issuer: bool,
	#[field(default = false)]
	pub cpu_autoscaling: bool,
	#[field(auto_now_add = true)]
	pub created_at: DateTime<Utc>,
}
