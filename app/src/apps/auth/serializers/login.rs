//! Login request serializer.

use reinhardt::{Schema, ToSchema, Validate};
use serde::Deserialize;

/// Login request body.
#[derive(Debug, Clone, Deserialize, Validate, Schema)]
pub struct LoginRequest {
	#[validate(length(min = 1))]
	pub username: String,
	#[validate(length(min = 1))]
	pub password: String,
}
