//! UserAccount persistence for the identity App.

use chrono::{DateTime, Utc};
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
