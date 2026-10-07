//! Persistence owned by the identity App.

use chrono::{DateTime, Utc};
use reinhardt::db::associations::ForeignKeyField;
use reinhardt::model;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[model(
	app_label = "identity",
	table_name = "cloud_identity_useraccount",
	info = false
)]
#[derive(Clone, Serialize, Deserialize)]
pub struct UserAccount {
	#[field(primary_key = true)]
	pub id: Uuid,
	#[field(max_length = 254, unique = true)]
	pub email: String,
	#[field(max_length = 512)]
	pub password_hash: String,
	#[field(default = true)]
	pub active: bool,
	#[field(default = false)]
	pub platform_admin: bool,
	#[field(auto_now_add = true)]
	pub created_at: DateTime<Utc>,
}

#[model(app_label = "identity", table_name = "cloud_session", info = false)]
#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
	#[field(primary_key = true)]
	pub id: Uuid,
	#[rel(foreign_key, on_delete = Cascade)]
	pub user: ForeignKeyField<UserAccount>,
	#[field(max_length = 128, unique = true)]
	pub token_hash: String,
	#[field]
	pub expires_at: DateTime<Utc>,
	#[field(default = false)]
	pub revoked: bool,
	#[field(auto_now_add = true)]
	pub created_at: DateTime<Utc>,
}
