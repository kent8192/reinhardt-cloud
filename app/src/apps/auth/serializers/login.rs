//! Login request serializer.

use reinhardt::Validate;
use serde::Deserialize;

/// Login request body.
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct LoginRequest {
	#[validate(length(min = 1))]
	pub username: String,
	#[validate(length(min = 1))]
	pub password: String,
}
