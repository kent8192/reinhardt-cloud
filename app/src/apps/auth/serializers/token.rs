//! Token response serializer.

use serde::Serialize;

/// JWT token response.
#[derive(Debug, Serialize)]
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
