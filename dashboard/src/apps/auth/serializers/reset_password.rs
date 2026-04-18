//! Request serializer for password reset.

use reinhardt::{Schema, ToSchema, Validate};
use serde::Deserialize;

/// Request body for the reset-password endpoint.
#[derive(Debug, Clone, Deserialize, Validate, Schema)]
pub struct ResetPasswordRequest {
	/// New password (minimum 8 characters).
	#[validate(length(min = 8, max = 128))]
	pub new_password: String,
}
