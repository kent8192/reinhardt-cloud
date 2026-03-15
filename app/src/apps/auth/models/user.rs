//! User model for auth app.

use chrono::{DateTime, Utc};
use reinhardt::prelude::*;
use reinhardt::{Argon2Hasher, BaseUser};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Nuages platform user account.
///
/// Minimal user model implementing `BaseUser` with Argon2id password hashing.
/// Unlike `DefaultUser`, this model omits permissions, groups, and staff flags
/// to keep the auth layer lightweight for a PaaS control plane.
///
/// Uses UUID v4 as the primary key for JWT `sub` claim compatibility
/// and to avoid sequential ID enumeration.
#[derive(Serialize, Deserialize)]
#[model(app_label = "auth", table_name = "auth_users")]
pub struct User {
	#[field(primary_key = true)]
	pub id: Uuid,

	#[field(max_length = 150, unique = true)]
	pub username: String,

	#[field(max_length = 254)]
	pub email: String,

	#[field(max_length = 512)]
	pub password_hash: Option<String>,

	#[field(default = true)]
	pub is_active: bool,

	#[field(include_in_new = false)]
	pub last_login: Option<DateTime<Utc>>,

	#[field(auto_now_add = true)]
	pub created_at: DateTime<Utc>,

	#[field(auto_now = true)]
	pub updated_at: DateTime<Utc>,
}

impl BaseUser for User {
	type PrimaryKey = Uuid;
	type Hasher = Argon2Hasher;

	fn get_username_field() -> &'static str {
		"username"
	}

	fn get_username(&self) -> &str {
		&self.username
	}

	fn password_hash(&self) -> Option<&str> {
		self.password_hash.as_deref()
	}

	fn set_password_hash(&mut self, hash: String) {
		self.password_hash = Some(hash);
	}

	fn last_login(&self) -> Option<DateTime<Utc>> {
		self.last_login
	}

	fn set_last_login(&mut self, time: DateTime<Utc>) {
		self.last_login = Some(time);
	}

	fn is_active(&self) -> bool {
		self.is_active
	}
}
