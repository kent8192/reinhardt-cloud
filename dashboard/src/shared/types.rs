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

#[cfg(not(target_arch = "wasm32"))]
impl From<&crate::apps::auth::models::User> for UserInfo {
	fn from(user: &crate::apps::auth::models::User) -> Self {
		use reinhardt::{BaseUser, FullUser};

		Self {
			id: user.id().to_string(),
			username: user.get_username().to_string(),
			email: user.email().to_string(),
		}
	}
}

/// Response from login/register server functions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
	/// Whether authentication was successful.
	pub success: bool,
	/// User information (present on success).
	pub user: Option<UserInfo>,
	/// Session token for subsequent authenticated requests.
	///
	/// The WASM client stores this token and includes it in future
	/// server function calls. Cookie-based session management is not
	/// available from within `#[server_fn]` handlers because the
	/// framework constructs the HTTP response externally.
	pub token: Option<String>,
}
