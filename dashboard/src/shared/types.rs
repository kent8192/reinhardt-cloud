//! Shared types used by both WASM client and server.

use serde::{Deserialize, Serialize};

/// User information returned after authentication.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
///
/// Authentication state is managed via HTTP-only session cookies set by
/// the server. The client does not need to handle tokens explicitly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthResponse {
	/// Whether authentication was successful.
	pub success: bool,
	/// User information (present on success).
	pub user: Option<UserInfo>,
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_user_info_serde_roundtrip() {
		// Arrange
		let user = UserInfo {
			id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
			username: "testuser".to_string(),
			email: "test@example.com".to_string(),
		};

		// Act
		let json = serde_json::to_string(&user).unwrap();
		let roundtrip: UserInfo = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(roundtrip.id, user.id);
		assert_eq!(roundtrip.username, user.username);
		assert_eq!(roundtrip.email, user.email);
	}

	#[rstest]
	fn test_auth_response_success() {
		// Arrange
		let response = AuthResponse {
			success: true,
			user: Some(UserInfo {
				id: "user-1".to_string(),
				username: "admin".to_string(),
				email: "admin@example.com".to_string(),
			}),
		};

		// Act
		let json = serde_json::to_string(&response).unwrap();
		let roundtrip: AuthResponse = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(roundtrip, response);
		assert!(roundtrip.success);
		assert!(roundtrip.user.is_some());
	}

	#[rstest]
	fn test_auth_response_failure() {
		// Arrange
		let response = AuthResponse {
			success: false,
			user: None,
		};

		// Act
		let json = serde_json::to_string(&response).unwrap();
		let roundtrip: AuthResponse = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(roundtrip, response);
		assert!(!roundtrip.success);
		assert!(roundtrip.user.is_none());
	}
}
