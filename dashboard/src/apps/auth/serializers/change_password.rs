//! Change password request serializer.

use reinhardt::{Schema, ToSchema, Validate};
use serde::Deserialize;

/// Change password request body for POST /auth/change-password/.
#[derive(Debug, Clone, Deserialize, Validate, Schema)]
pub struct ChangePasswordRequest {
	#[validate(length(min = 1, max = 128))]
	pub old_password: String,
	#[validate(length(min = 8, max = 128))]
	pub new_password: String,
}
