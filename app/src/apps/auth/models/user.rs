//! User model for auth app.

use chrono::{DateTime, Utc};
use reinhardt::Argon2Hasher;
use reinhardt::macros::user;
use reinhardt::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Reinhardt Cloud platform user account.
///
/// Uses `#[user]` macro with `full = true` to implement `BaseUser`,
/// `FullUser`, `PermissionsMixin`, and `AuthIdentity` traits automatically.
/// This enables admin panel access via the `AdminUser` blanket impl.
///
/// Uses UUID v4 as the primary key for JWT `sub` claim compatibility
/// and to avoid sequential ID enumeration.
#[user(hasher = Argon2Hasher, username_field = "username", full = true)]
#[derive(Serialize, Deserialize)]
#[model(app_label = "auth", table_name = "auth_users")]
pub struct User {
	#[field(primary_key = true, include_in_new = false)]
	pub id: Uuid,

	#[field(max_length = 150, unique = true)]
	pub username: String,

	#[field(max_length = 254, unique = true)]
	pub email: String,

	#[field(max_length = 128, default = "")]
	pub first_name: String,

	#[field(max_length = 128, default = "")]
	pub last_name: String,

	#[field(max_length = 512)]
	pub password_hash: Option<String>,

	#[field(default = true)]
	pub is_active: bool,

	#[field(default = false)]
	pub is_staff: bool,

	#[field(default = false)]
	pub is_superuser: bool,

	#[field(include_in_new = false)]
	pub last_login: Option<DateTime<Utc>>,

	#[field(auto_now_add = true)]
	pub date_joined: DateTime<Utc>,

	#[field(auto_now = true)]
	pub updated_at: DateTime<Utc>,

	#[serde(default)]
	pub user_permissions: Vec<String>,

	#[serde(default)]
	pub groups: Vec<String>,
}
