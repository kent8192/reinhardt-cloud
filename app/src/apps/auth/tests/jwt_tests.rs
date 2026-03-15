//! Tests for auth JWT integration.

#[cfg(test)]
mod tests {
	use reinhardt::JwtAuth;
	use rstest::rstest;

	use crate::apps::auth::serializers::TokenResponse;

	#[rstest]
	fn test_jwt_generate_and_verify() {
		// Arrange
		let auth = JwtAuth::new(b"test-secret-minimum-32-bytes-long!!");

		// Act
		let token = auth
			.generate_token("user-123".to_string(), "testuser".to_string())
			.unwrap();
		let claims = auth.verify_token(&token).unwrap();

		// Assert
		assert_eq!(claims.sub, "user-123");
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
}
