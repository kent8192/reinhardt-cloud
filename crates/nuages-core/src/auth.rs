//! JWT authentication utilities for the nuages platform.

use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// JWT claims for nuages platform authentication.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
	/// Subject (user ID)
	pub sub: String,
	/// Username
	pub username: String,
	/// Expiration time (Unix timestamp)
	pub exp: i64,
	/// Issued at (Unix timestamp)
	pub iat: i64,
}

/// Creates a signed JWT token for the given user.
pub fn create_token(
	user_id: Uuid,
	username: &str,
	secret: &[u8],
	expiry_hours: i64,
) -> Result<String, jsonwebtoken::errors::Error> {
	let now = Utc::now();
	let claims = Claims {
		sub: user_id.to_string(),
		username: username.to_string(),
		exp: (now + Duration::hours(expiry_hours)).timestamp(),
		iat: now.timestamp(),
	};
	encode(
		&Header::default(),
		&claims,
		&EncodingKey::from_secret(secret),
	)
}

/// Verifies and decodes a JWT token.
pub fn verify_token(token: &str, secret: &[u8]) -> Result<Claims, jsonwebtoken::errors::Error> {
	let token_data = decode::<Claims>(
		token,
		&DecodingKey::from_secret(secret),
		&Validation::default(),
	)?;
	Ok(token_data.claims)
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	const TEST_SECRET: &[u8] = b"test-secret-key-for-jwt-signing";

	#[rstest]
	fn test_create_and_verify_token_roundtrip() {
		// Arrange
		let user_id = Uuid::new_v4();
		let username = "alice";

		// Act
		let token = create_token(user_id, username, TEST_SECRET, 24).unwrap();
		let claims = verify_token(&token, TEST_SECRET).unwrap();

		// Assert
		assert_eq!(claims.sub, user_id.to_string());
		assert_eq!(claims.username, username);
		assert!(claims.exp > claims.iat);
	}

	#[rstest]
	fn test_verify_token_with_wrong_secret() {
		// Arrange
		let user_id = Uuid::new_v4();
		let token = create_token(user_id, "bob", TEST_SECRET, 24).unwrap();

		// Act
		let result = verify_token(&token, b"wrong-secret");

		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	fn test_expired_token_is_rejected() {
		// Arrange
		let user_id = Uuid::new_v4();
		// Create a token that expired 1 hour ago
		let token = create_token(user_id, "charlie", TEST_SECRET, -1).unwrap();

		// Act
		let result = verify_token(&token, TEST_SECRET);

		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	fn test_claims_contain_correct_fields() {
		// Arrange
		let user_id = Uuid::new_v4();
		let username = "dave";

		// Act
		let token = create_token(user_id, username, TEST_SECRET, 8).unwrap();
		let claims = verify_token(&token, TEST_SECRET).unwrap();

		// Assert
		assert_eq!(claims.sub, user_id.to_string());
		assert_eq!(claims.username, "dave");
		assert!(claims.iat > 0);
		assert!(claims.exp > claims.iat);
	}
}
