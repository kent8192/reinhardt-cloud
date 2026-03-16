//! Register request serializer.

use reinhardt::Validate;
use serde::Deserialize;

/// User registration request body.
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct RegisterRequest {
	#[validate(length(min = 3, max = 32))]
	pub username: String,
	#[validate(email)]
	pub email: String,
	#[validate(length(min = 8))]
	pub password: String,
}
