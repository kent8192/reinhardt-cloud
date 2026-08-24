//! Tests for SocialAccount model construction and serialization.

#[cfg(test)]
mod tests {
	use chrono::Utc;
	use reinhardt::db::migrations::ForeignKeyAction;
	use reinhardt::db::migrations::operations::{Constraint, Operation};
	use rstest::rstest;
	use uuid::Uuid;

	use crate::apps::auth::models::SocialAccount;

	mod auth_initial_migration {
		include!(concat!(
			env!("CARGO_MANIFEST_DIR"),
			"/migrations/auth/0001_initial.rs"
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
		assert_eq!(account.user_id(), Uuid::nil());
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
	fn test_auth_initial_migration_contains_final_social_account_token_columns() {
		// Arrange
		let migration = auth_initial_migration::migration();

		// Act
		let columns = migration
			.operations
			.iter()
			.find_map(|operation| match operation {
				Operation::CreateTable { name, columns, .. } if name == "auth_social_accounts" => {
					Some(columns)
				}
				_ => None,
			})
			.expect("auth_social_accounts table must be created");
		let token_columns = columns
			.iter()
			.filter(|column| {
				matches!(
					column.name.as_str(),
					"encrypted_access_token" | "token_expires_at" | "scopes"
				)
			})
			.map(|column| {
				(
					column.name.as_str(),
					format!("{:?}", column.type_definition),
					column.not_null,
					column.unique,
					column.primary_key,
					column.auto_increment,
					column.default.as_deref(),
				)
			})
			.collect::<Vec<_>>();

		// Assert
		assert_eq!(migration.app_label, "auth");
		assert_eq!(migration.name, "0001_initial");
		assert_eq!(
			token_columns,
			vec![
				(
					"encrypted_access_token",
					"VarChar(4096)".to_string(),
					false,
					false,
					false,
					false,
					None,
				),
				(
					"scopes",
					"VarChar(2048)".to_string(),
					false,
					false,
					false,
					false,
					None,
				),
				(
					"token_expires_at",
					"TimestampTz".to_string(),
					false,
					false,
					false,
					false,
					None,
				),
			]
		);
	}

	#[rstest]
	fn test_auth_initial_migration_preserves_constraints_and_active_token_index() {
		// Arrange
		let migration = auth_initial_migration::migration();

		// Act
		let social_unique_constraints = migration
			.operations
			.iter()
			.find_map(|operation| match operation {
				Operation::CreateTable {
					name, constraints, ..
				} if name == "auth_social_accounts" => Some(
					constraints
						.iter()
						.filter_map(|constraint| match constraint {
							Constraint::Unique { name, columns } => {
								Some((name.clone(), columns.clone()))
							}
							_ => None,
						})
						.collect::<Vec<_>>(),
				),
				_ => None,
			})
			.expect("auth_social_accounts table must be created");
		let api_key_user_action =
			migration
				.operations
				.iter()
				.find_map(|operation| match operation {
					Operation::CreateTable {
						name, constraints, ..
					} if name == "auth_api_keys" => constraints.iter().find_map(|constraint| match constraint {
						Constraint::ForeignKey {
							columns,
							on_delete,
							on_update,
							..
						} if columns == &["user_id"] => Some((*on_delete, *on_update)),
						_ => None,
					}),
					_ => None,
				});
		let email_token_indexes = migration
			.operations
			.iter()
			.filter_map(|operation| match operation {
				Operation::CreateIndex {
					table,
					columns,
					unique,
					where_clause,
					..
				} if table == "auth_email_verification_tokens" => {
					Some((columns.clone(), *unique, where_clause.clone()))
				}
				_ => None,
			})
			.collect::<Vec<_>>();

		// Assert
		assert_eq!(
			social_unique_constraints,
			vec![(
				"auth_social_account_provider_uid_uniq".to_string(),
				vec!["provider".to_string(), "provider_user_id".to_string()],
			)]
		);
		assert_eq!(
			api_key_user_action,
			Some((ForeignKeyAction::Cascade, ForeignKeyAction::NoAction))
		);
		assert_eq!(
			email_token_indexes,
			vec![(
				vec!["user_id".to_string()],
				false,
				Some("consumed_at IS NULL".to_string()),
			)]
		);
	}
}
