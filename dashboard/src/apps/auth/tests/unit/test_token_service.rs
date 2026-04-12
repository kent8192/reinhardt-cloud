//! Unit tests for the HMAC token service.
//!
//! The primary token tests live as inline `#[cfg(test)]` in
//! `services/token.rs`. This module provides additional coverage
//! using the project's `settings.core.secret_key` to verify
//! integration with the actual settings system.

#[cfg(test)]
mod tests {
	use rstest::*;
	use uuid::Uuid;

	use crate::apps::auth::services::token::{
		TokenError, TokenPurpose, generate_token, verify_token,
	};

	fn test_secret() -> String {
		crate::config::settings::get_settings()
			.core
			.secret_key
			.clone()
	}

	#[rstest]
	fn test_token_roundtrip_with_project_secret() {
		// Arrange
		let user_id = Uuid::new_v4();
		let secret = test_secret();

		// Act
		let token = generate_token(TokenPurpose::EmailVerification, &user_id, "", &secret);
		let result = verify_token(&token, TokenPurpose::EmailVerification, "", &secret);

		// Assert
		assert_eq!(result, Ok(user_id));
	}

	#[rstest]
	fn test_password_reset_token_with_project_secret() {
		// Arrange
		let user_id = Uuid::new_v4();
		let secret = test_secret();
		let hash = "$argon2id$v=19$m=19456,t=2,p=1$some-salt$some-hash";

		// Act
		let token = generate_token(TokenPurpose::PasswordReset, &user_id, hash, &secret);
		let result = verify_token(&token, TokenPurpose::PasswordReset, hash, &secret);

		// Assert
		assert_eq!(result, Ok(user_id));
	}

	#[rstest]
	fn test_cross_purpose_rejection() {
		// Arrange
		let user_id = Uuid::new_v4();
		let secret = test_secret();
		let token = generate_token(TokenPurpose::EmailVerification, &user_id, "", &secret);

		// Act
		let result = verify_token(&token, TokenPurpose::PasswordReset, "", &secret);

		// Assert
		assert_eq!(result, Err(TokenError::PurposeMismatch));
	}
}
