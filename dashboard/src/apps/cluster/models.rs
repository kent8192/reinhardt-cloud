//! Persistence owned by the cluster App.

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

/// An Environment has one runtime allocation with a bounded replica budget.
#[model(
	app_label = "cluster",
	table_name = "cloud_environment_allocation",
	info = false
)]
#[derive(Clone, Serialize, Deserialize)]
pub struct EnvironmentAllocation {
	#[field(primary_key = true)]
	pub id: Uuid,
	#[rel(one_to_one, on_delete = Cascade)]
	pub environment:
		reinhardt::db::associations::OneToOneField<crate::apps::project::models::Environment>,
	#[rel(foreign_key, on_delete = Restrict)]
	pub cluster: reinhardt::db::associations::ForeignKeyField<Cluster>,
	#[field(default = 10)]
	pub replica_limit: i32,
}
