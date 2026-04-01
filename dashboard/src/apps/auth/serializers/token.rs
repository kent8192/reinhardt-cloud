//! Token response serializer.

use reinhardt::{Schema, ToSchema};
use serde::Serialize;

/// JWT token response.
#[derive(Debug, Serialize, Schema)]
pub struct TokenResponse {
	pub token: String,
	pub token_type: String,
}

impl TokenResponse {
	/// Creates a Bearer token response.
	pub fn bearer(token: String) -> Self {
		Self {
			token,
			token_type: "Bearer".to_string(),
		}
	}
}
