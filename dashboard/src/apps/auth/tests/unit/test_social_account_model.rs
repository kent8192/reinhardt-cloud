//! Tests for SocialAccount model construction and serialization.

#[cfg(test)]
mod tests {
	use chrono::Utc;
	use reinhardt::db::migrations::operations::Operation;
	use rstest::rstest;
	use uuid::Uuid;

	use crate::apps::auth::models::SocialAccount;

	// Included migration files keep `pub fn migration()` because production
	// discovery loads that symbol from standalone migration modules.
	#[allow(unreachable_pub)]
	mod token_metadata_migration {
		include!(concat!(
			env!("CARGO_MANIFEST_DIR"),
			"/migrations/auth/0006_add_social_account_token_metadata.rs"
		));
	}

	fn sample_account() -> SocialAccount {
		let now = Utc::now();
		let mut account = SocialAccount::build()
			.user(Uuid::new_v4())
			.provider("github".to_string())
			.provider_user_id("12345".to_string())
			.provider_username(Some("octocat".to_string()))
			.encrypted_access_token(None)
			.token_expires_at(None)
			.scopes(None)
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
		assert!(account.encrypted_access_token.is_none());
		assert!(account.token_expires_at.is_none());
		assert!(account.scopes.is_none());
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
		assert_eq!(
			deserialized.encrypted_access_token,
			account.encrypted_access_token
		);
		assert_eq!(deserialized.token_expires_at, account.token_expires_at);
		assert_eq!(deserialized.scopes, account.scopes);
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

	#[rstest]
	fn test_social_account_token_metadata_migration_adds_nullable_columns() {
		// Arrange
		let migration = token_metadata_migration::migration();

		// Act & Assert
		assert_add_column(
			&migration.operations,
			"encrypted_access_token",
			"VarChar(4096)",
		);
		assert_add_column(&migration.operations, "token_expires_at", "TimestampTz");
		assert_add_column(&migration.operations, "scopes", "VarChar(2048)");
		assert_eq!(
			migration.dependencies,
			vec![("auth".to_string(), "0005_add_social_accounts".to_string())]
		);
	}

	fn assert_add_column(operations: &[Operation], name: &str, field_type: &str) {
		let column = operations
			.iter()
			.find_map(|operation| match operation {
				Operation::AddColumn { table, column, .. }
					if table == "auth_social_accounts" && column.name == name =>
				{
					Some(column)
				}
				_ => None,
			})
			.unwrap_or_else(|| panic!("{name} column must be added"));
		assert_eq!(format!("{:?}", column.type_definition), field_type);
		assert!(!column.not_null, "{name}.not_null");
		assert!(!column.unique, "{name}.unique");
		assert!(!column.primary_key, "{name}.primary_key");
		assert!(!column.auto_increment, "{name}.auto_increment");
		assert!(column.default.is_none(), "{name}.default");
	}
}
