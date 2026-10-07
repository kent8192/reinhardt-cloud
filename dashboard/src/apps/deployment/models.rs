//! Persistence owned by the deployment App.

use crate::apps::organization::models::Organization;
use crate::apps::project::models::Environment;
use chrono::{DateTime, Utc};
use reinhardt::db::associations::ForeignKeyField;
use reinhardt::model;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[model(app_label = "deployment", table_name = "cloud_deployment_operation", info = false, unique_together = ("organization_id", "environment_id", "idempotency_key"))]
#[derive(Clone, Serialize, Deserialize)]
pub struct Operation {
	#[field(primary_key = true)]
	pub id: Uuid,
	#[rel(foreign_key, on_delete = Restrict)]
	pub organization: ForeignKeyField<Organization>,
	#[rel(foreign_key, on_delete = Restrict)]
	pub environment: ForeignKeyField<Environment>,
	#[field]
	pub environment_version: i64,
	#[field(max_length = 200)]
	pub idempotency_key: String,
	#[field(max_length = 64, default = "")]
	pub request_fingerprint: String,
	#[field(max_length = 32)]
	pub state: String,
	#[field(max_length = 32)]
	pub kind: String,
	#[field(auto_now_add = true)]
	pub created_at: DateTime<Utc>,
}

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
