//! SecretVersion persistence for the secret App.

use crate::apps::project::models::Environment;
use chrono::{DateTime, Utc};
use reinhardt::db::associations::ForeignKeyField;
use reinhardt::model;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[model(
	app_label = "secret",
	table_name = "cloud_secret_secretversion",
	info = false
)]
#[derive(Clone, Serialize, Deserialize)]
pub struct SecretVersion {
	#[field(primary_key = true)]
	pub id: Uuid,
	#[rel(foreign_key, on_delete = Restrict)]
	pub environment: ForeignKeyField<Environment>,
	#[field(max_length = 200)]
	pub name: String,
	// Workaround for kent8192/reinhardt-web#6637 (tracked in reinhardt-cloud#912).
	// Remove after an approved framework upgrade includes PostgreSQL Binary -> BYTEA
	// rendering and native migration application passes. This stores encoded ciphertext.
	// Ideal implementation (without workaround):
	//   #[field]
	//   pub ciphertext: Vec<u8>,
	#[field(field_type = "text")]
	pub ciphertext: String,
	#[field(max_length = 200)]
	pub key_id: String,
	#[field(default = false)]
	pub revoked: bool,
	#[field(auto_now_add = true)]
	pub created_at: DateTime<Utc>,
}
