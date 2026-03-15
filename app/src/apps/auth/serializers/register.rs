//! Register request serializer.

use serde::Deserialize;

/// User registration request body.
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
	pub username: String,
	pub email: String,
	pub password: String,
}
