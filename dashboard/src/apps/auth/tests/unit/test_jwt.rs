//! Tests for auth JWT integration.

#[cfg(test)]
mod tests {
	use reinhardt::JwtAuth;
	use rstest::rstest;
	use uuid::Uuid;

	use crate::apps::auth::serializers::TokenResponse;

	#[rstest]
	fn test_jwt_generate_and_verify_with_uuid() {
		// Arrange
		let auth = JwtAuth::new(b"test-secret-minimum-32-bytes-long!!");
		let user_id = Uuid::new_v4().to_string();

		// Act
		let token = auth
			.generate_token(user_id.clone(), "testuser".to_string())
			.unwrap();
		let claims = auth.verify_token(&token).unwrap();

		// Assert
		assert_eq!(claims.sub, user_id);
		assert_eq!(claims.username, "testuser");
		assert!(!claims.is_expired());
	}

	#[rstest]
	fn test_token_response_bearer() {
		// Arrange
		let token = "eyJhbGciOiJIUzI1NiJ9.test".to_string();

		// Act
		let resp = TokenResponse::bearer(token.clone());

		// Assert
		assert_eq!(resp.token, token);
		assert_eq!(resp.token_type, "Bearer");
	}

	#[rstest]
	fn test_password_hash_and_verify() {
		// Arrange
		use crate::apps::auth::models::User;
		use reinhardt::BaseUser;

		let mut user = User::new(
			"hashtest".to_string(),
			"hash@test.com".to_string(),
			String::new(),
			String::new(),
			None,
			true,
			false,
			false,
		);

		// Act
		user.set_password("secure-password-123").unwrap();

		// Assert
		assert!(user.password_hash().is_some());
		assert!(user.check_password("secure-password-123").unwrap());
		assert!(!user.check_password("wrong-password").unwrap());
	}
}
