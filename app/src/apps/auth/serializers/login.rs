//! Login request serializer.

use serde::Deserialize;

/// Login request body.
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
	pub username: String,
	pub password: String,
}
