//! DispatchRecord persistence for the deployment App.

use super::Operation;
use crate::apps::project::models::Environment;
use reinhardt::db::associations::ForeignKeyField;
use reinhardt::model;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[model(
	app_label = "deployment",
	table_name = "cloud_dispatch_record",
	info = false
)]
#[derive(Clone, Serialize, Deserialize)]
pub struct DispatchRecord {
	#[field(primary_key = true)]
	pub id: Uuid,
	#[rel(one_to_one, on_delete = Cascade)]
	pub operation: reinhardt::db::associations::OneToOneField<Operation>,
	#[rel(foreign_key, on_delete = Restrict)]
	pub environment: ForeignKeyField<Environment>,
	#[field]
	pub environment_version: i64,
	#[field(max_length = 65535)]
	pub payload: String,
	#[rel(foreign_key, on_delete = Restrict)]
	pub cluster: ForeignKeyField<crate::apps::cluster::models::Cluster>,
	#[field(default = false)]
	pub delivered: bool,
}
