//! Tests for SocialAccount model construction and serialization.

#[cfg(test)]
mod tests {
	use chrono::Utc;
	use rstest::rstest;
	use uuid::Uuid;

	use crate::apps::auth::models::SocialAccount;

	fn sample_account() -> SocialAccount {
		let now = Utc::now();
		let mut account = SocialAccount::build()
			.user(Uuid::new_v4())
			.provider("github".to_string())
			.provider_user_id("12345".to_string())
			.provider_username(Some("octocat".to_string()))
			.finish();
		account.id = Uuid::new_v4();
		account.created_at = now;
		account.updated_at = now;
		account
	}

	#[rstest]
	fn test_social_account_default_is_zeroed() {
		// Arrange & Act
		let account = SocialAccount::default();

		// Assert
		assert_eq!(account.id, Uuid::nil());
		assert_eq!(*account.user_id(), Uuid::nil());
		assert!(account.provider.is_empty());
		assert!(account.provider_user_id.is_empty());
		assert!(account.provider_username.is_none());
	}

	#[rstest]
	fn test_social_account_serialization_roundtrip() {
		// Arrange
		let account = sample_account();

		// Act
		let json = serde_json::to_string(&account).expect("Failed to serialize SocialAccount");
		let deserialized: SocialAccount =
			serde_json::from_str(&json).expect("Failed to deserialize SocialAccount");

		// Assert
		assert_eq!(deserialized.id, account.id);
		assert_eq!(deserialized.user_id(), account.user_id());
		assert_eq!(deserialized.provider, account.provider);
		assert_eq!(deserialized.provider_user_id, account.provider_user_id);
		assert_eq!(deserialized.provider_username, account.provider_username);
	}

	#[rstest]
	fn test_social_account_provider_username_optional() {
		// Arrange
		let mut account = sample_account();
		account.provider_username = None;

		// Act
		let json = serde_json::to_string(&account).expect("Failed to serialize");
		let deserialized: SocialAccount =
			serde_json::from_str(&json).expect("Failed to deserialize");

		// Assert
		assert!(deserialized.provider_username.is_none());
	}

	#[rstest]
	#[case("github")]
	#[case("gitlab")]
	fn test_social_account_accepts_known_providers(#[case] provider: &str) {
		// Arrange
		let mut account = sample_account();
		account.provider = provider.to_string();

		// Act
		let json = serde_json::to_string(&account).expect("Failed to serialize");
		let deserialized: SocialAccount =
			serde_json::from_str(&json).expect("Failed to deserialize");

		// Assert
		assert_eq!(deserialized.provider, provider);
	}
}
