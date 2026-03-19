//! Shared types used by both WASM client and server.

use serde::{Deserialize, Serialize};

/// User information returned after authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
	/// User's unique identifier (UUID as string).
	pub id: String,
	/// Username.
	pub username: String,
	/// Email address.
	pub email: String,
}

/// Response from login/register server functions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
	/// Whether authentication was successful.
	pub success: bool,
	/// User information (present on success).
	pub user: Option<UserInfo>,
}
