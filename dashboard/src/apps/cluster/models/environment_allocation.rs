//! EnvironmentAllocation persistence for the cluster App.

use super::Cluster;
use reinhardt::model;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
