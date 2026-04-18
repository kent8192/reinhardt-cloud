//! Request serializer for forgot-password flow.

use reinhardt::{Schema, ToSchema, Validate};
use serde::Deserialize;

/// Request body for the forgot-password endpoint.
#[derive(Debug, Clone, Deserialize, Validate, Schema)]
pub struct ForgotPasswordRequest {
	/// Email address of the account to reset.
	#[validate(email, length(max = 254))]
	pub email: String,
}
