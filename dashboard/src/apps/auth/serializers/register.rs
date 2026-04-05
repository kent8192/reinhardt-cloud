//! Register request serializer.

use reinhardt::{Schema, ToSchema, Validate};
use serde::Deserialize;

/// User registration request body.
#[derive(Debug, Clone, Deserialize, Validate, Schema)]
pub struct RegisterRequest {
	#[validate(length(min = 3, max = 32))]
	pub username: String,
	#[validate(email, length(max = 254))]
	pub email: String,
	#[validate(length(min = 8, max = 128))]
	pub password: String,
}
