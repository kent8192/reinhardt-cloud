//! Tests for session token creation and validation.

#[cfg(test)]
mod tests {
	use reinhardt::{BaseUser, JwtAuth};
	use rstest::rstest;
	use serial_test::serial;
	use uuid::Uuid;

	use crate::apps::auth::models::User;
	use crate::apps::auth::services::session::{create_session_token, validate_raw_token};

	/// Retrieve the effective JWT secret used by the session service.
	///
	/// The service reads from TOML settings first, then falls back to env var.
	/// Tests must use the same secret the service will use.
	fn effective_jwt_secret() -> String {
		crate::config::settings::get_jwt_secret()
			.unwrap_or_else(|| "test-secret-minimum-32-bytes-long!!".to_string())
	}

	/// Helper: set the JWT secret env var used by session service.
	fn set_jwt_secret(secret: &str) {
		unsafe {
			std::env::set_var("REINHARDT_CLOUD_JWT_SECRET", secret);
		}
	}

	#[rstest]
	#[serial(jwt)]
	fn test_create_session_token_returns_valid_jwt() {
		// Arrange — ensure env var is set (TOML takes priority if present)
		set_jwt_secret(&effective_jwt_secret());
		let mut user = User::new(
			"sessionuser".to_string(),
			"session@example.com".to_string(),
			String::new(),
			String::new(),
			None,
			true,
			false,
			false,
		);
		user.set_password("password123").unwrap();

		// Act
		let token = create_session_token(&user).expect("create_session_token should succeed");

		// Assert — validate_raw_token should return matching user info
		let (user_id, username) = validate_raw_token(&token).expect("Token should be valid");
		assert_eq!(user_id, user.id().to_string());
		assert_eq!(username, "sessionuser");
	}

	#[rstest]
	#[serial(jwt)]
	fn test_validate_raw_token_with_valid_token() {
		// Arrange — use the effective secret (TOML takes priority over env var)
		let secret = effective_jwt_secret();
		let user_id = Uuid::new_v4().to_string();
		let auth = JwtAuth::new(secret.as_bytes());
		let token = auth
			.generate_token(user_id.clone(), "manualuser".to_string(), false, false)
			.expect("Token generation should succeed");

		// Act
		let result = validate_raw_token(&token);

		// Assert
		let (sub, username) = result.expect("Valid token should be accepted");
		assert_eq!(sub, user_id);
		assert_eq!(username, "manualuser");
	}

	#[rstest]
	#[serial(jwt)]
	fn test_validate_raw_token_empty_string() {
		// Arrange
		set_jwt_secret(&effective_jwt_secret());

		// Act
		let result = validate_raw_token("");

		// Assert
		assert!(result.is_none(), "Empty string should return None");
	}

	#[rstest]
	#[serial(jwt)]
	fn test_validate_raw_token_garbage() {
		// Arrange
		set_jwt_secret(&effective_jwt_secret());

		// Act
		let result = validate_raw_token("not-a-jwt");

		// Assert
		assert!(result.is_none(), "Garbage string should return None");
	}

	#[rstest]
	#[serial(jwt)]
	fn test_validate_raw_token_wrong_secret() {
		// Arrange — generate token with one secret
		let auth = JwtAuth::new(b"original-secret-at-least-32-bytes!!");
		let token = auth
			.generate_token(
				Uuid::new_v4().to_string(),
				"wrongsecret".to_string(),
				false,
				false,
			)
			.expect("Token generation should succeed");

		// Set a different secret in the environment
		set_jwt_secret("different-secret-at-least-32-bytes!");

		// Act
		let result = validate_raw_token(&token);

		// Assert
		assert!(
			result.is_none(),
			"Token signed with wrong secret should return None"
		);
	}

	#[rstest]
	#[serial(jwt)]
	fn test_validate_raw_token_tampered_payload() {
		// Arrange — generate a valid token, then tamper with the payload segment
		let secret = effective_jwt_secret();
		set_jwt_secret(&secret);
		let auth = JwtAuth::new(secret.as_bytes());
		let token = auth
			.generate_token(
				Uuid::new_v4().to_string(),
				"tampered".to_string(),
				false,
				false,
			)
			.expect("Token generation should succeed");

		// Tamper: replace the middle (payload) segment
		let parts: Vec<&str> = token.split('.').collect();
		assert_eq!(parts.len(), 3, "JWT should have 3 parts");
		let tampered = format!("{}.dGFtcGVyZWQ.{}", parts[0], parts[2]);

		// Act
		let result = validate_raw_token(&tampered);

		// Assert
		assert!(result.is_none(), "Tampered token should return None");
	}
}
