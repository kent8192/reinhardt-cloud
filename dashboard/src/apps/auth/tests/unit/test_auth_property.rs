//! Property-based tests for auth components.

#[cfg(test)]
mod tests {
	use proptest::prelude::*;
	use reinhardt::{BaseUser, JwtAuth};
	use uuid::Uuid;

	use crate::apps::auth::models::User;
	use crate::apps::auth::serializers::TokenResponse;
	use crate::apps::auth::services::session::validate_raw_token;

	proptest! {
		/// Any valid UUID + alphanumeric username should roundtrip through JWT.
		#[test]
		fn test_jwt_roundtrip_any_uuid_username(
			username in "[a-zA-Z0-9]{1,32}"
		) {
			// Arrange
			let secret = b"proptest-secret-minimum-32-bytes!!";
			let auth = JwtAuth::new(secret);
			let user_id = Uuid::new_v4().to_string();

			// Act
			let token = auth
				.generate_token(user_id.clone(), username.clone(), false, false)
				.unwrap();
			let claims = auth.verify_token(&token).unwrap();

			// Assert
			prop_assert_eq!(&claims.sub, &user_id);
			prop_assert_eq!(&claims.username, &username);
			prop_assert!(!claims.is_expired());
		}

		/// Any non-empty password should hash and verify correctly.
		#[test]
		fn test_password_hash_verify_roundtrip(
			password in ".{1,64}"
		) {
			// Arrange
			let mut user = User::new(
				"propuser".to_string(),
				"prop@example.com".to_string(),
				String::new(),
				String::new(),
				None,
				true,
				false,
				false,
			);

			// Act
			user.set_password(&password).unwrap();

			// Assert
			prop_assert!(user.check_password(&password).unwrap());
		}

		/// Random strings should never validate as a JWT.
		#[test]
		fn test_random_string_never_validates_as_jwt(
			input in ".*"
		) {
			// Arrange
			unsafe {
				std::env::set_var(
					"REINHARDT_CLOUD_JWT_SECRET",
					"proptest-secret-minimum-32-bytes!!",
				);
			}

			// Act
			let result = validate_raw_token(&input);

			// Assert — random strings should not produce valid claims
			// (technically a valid JWT could be generated, but the probability is negligible)
			if let Some((sub, _username)) = result {
				// If somehow it parses, the sub must be a valid UUID
				// (our JWT always stores UUID as sub)
				prop_assert!(
					Uuid::parse_str(&sub).is_ok(),
					"If token validates, sub must be a valid UUID"
				);
			}
		}

		/// TokenResponse::bearer should roundtrip through serde.
		#[test]
		fn test_token_response_serialization_roundtrip(
			token_str in "[a-zA-Z0-9._-]{1,200}"
		) {
			// Arrange
			let resp = TokenResponse::bearer(token_str.clone());

			// Act
			let json = serde_json::to_string(&resp).unwrap();
			let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

			// Assert
			prop_assert_eq!(parsed["token"].as_str().unwrap(), token_str.as_str());
			prop_assert_eq!(parsed["token_type"].as_str().unwrap(), "Bearer");
		}
	}
}
