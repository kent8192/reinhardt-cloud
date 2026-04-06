//! Profile request/response serializers.

use chrono::{DateTime, Utc};
use reinhardt::{Schema, ToSchema, Validate};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::apps::auth::models::User;

/// Profile response returned by GET /auth/profile/.
#[derive(Debug, Serialize, Schema)]
pub struct ProfileResponse {
	pub id: Uuid,
	pub username: String,
	pub email: String,
	pub first_name: String,
	pub last_name: String,
	pub is_active: bool,
	pub is_staff: bool,
	pub is_superuser: bool,
	pub date_joined: DateTime<Utc>,
	pub updated_at: DateTime<Utc>,
}

impl From<User> for ProfileResponse {
	fn from(u: User) -> Self {
		Self {
			id: u.id,
			username: u.username,
			email: u.email,
			first_name: u.first_name,
			last_name: u.last_name,
			is_active: u.is_active,
			is_staff: u.is_staff,
			is_superuser: u.is_superuser,
			date_joined: u.date_joined,
			updated_at: u.updated_at,
		}
	}
}

/// Update profile request body for PATCH /auth/profile/.
#[derive(Debug, Clone, Deserialize, Validate, Schema)]
pub struct UpdateProfileRequest {
	#[validate(length(min = 1, max = 150))]
	pub first_name: Option<String>,
	#[validate(length(min = 1, max = 150))]
	pub last_name: Option<String>,
	#[validate(email, length(max = 254))]
	pub email: Option<String>,
}
