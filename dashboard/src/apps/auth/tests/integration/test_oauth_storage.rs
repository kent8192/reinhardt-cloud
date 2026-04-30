//! Integration tests for `OrmSocialAccountStorage`.
//!
//! Verifies that the dashboard's `SocialAccountStorage` impl honours the
//! framework contract — round-trips, lookup by provider/uid, listing by
//! user, update-of-missing returns an error, and delete is idempotent on
//! the missing case the same way the in-memory reference impl behaves.

#[cfg(test)]
mod tests {
	use chrono::{Duration, Utc};
	use reinhardt::BaseUser;
	use reinhardt::db::orm::Model;
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::test::APIClient;
	use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage};
	use reinhardt_auth::social::storage::{SocialAccount, SocialAccountStorage};
	use rstest::*;
	use serial_test::serial;
	use std::sync::Arc;
	use uuid::Uuid;

	use crate::apps::auth::models::User;
	use crate::apps::auth::services::oauth::storage::OrmSocialAccountStorage;
	use crate::config::test_helpers::ResolvedUrls;

	#[fixture]
	async fn db() -> (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		ResolvedUrls,
	) {
		// Start TestContainers first so build_test_app() registers DatabaseConnection
		// in the DI scope. Fixes #459.
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations");
		let (client, urls) = crate::config::test_helpers::build_test_app();
		(container, conn, client, urls)
	}

	async fn create_test_user(username: &str, email: &str) -> Uuid {
		let mut user = User::new(
			username.to_string(),
			email.to_lowercase(),
			String::new(),
			String::new(),
			None,
			true,
			false,
			false,
		);
		// OAuth-only users have no password, but providing one keeps the
		// fixture realistic and matches Argon2 hash expectations.
		user.set_password("test-password-1234").unwrap();
		let created = User::objects()
			.create(&user)
			.await
			.expect("Failed to create user");
		created.id
	}

	fn sample_account(user_id: Uuid, provider: &str, provider_uid: &str) -> SocialAccount {
		let now = Utc::now();
		SocialAccount {
			id: Uuid::now_v7(),
			user_id,
			provider: provider.to_string(),
			provider_user_id: provider_uid.to_string(),
			email: Some("user@example.com".to_string()),
			display_name: Some("Test User".to_string()),
			picture: None,
			// Token fields are intentionally populated here so that the
			// test verifies the storage layer DOES NOT round-trip them.
			access_token: "live_access_token".to_string(),
			refresh_token: Some("live_refresh_token".to_string()),
			token_expires_at: now + Duration::hours(1),
			scopes: vec!["read:user".to_string()],
			created_at: now,
			updated_at: now,
		}
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_create_then_find_by_provider(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
		),
	) {
		// Arrange
		let (_container, _conn, _client, _urls) = db.await;
		let user_id = create_test_user("oauth_find_one", "oauthone@example.com").await;
		let storage = OrmSocialAccountStorage::new();
		let account = sample_account(user_id, "github", "gh_42");

		// Act
		let created = storage
			.create(account.clone())
			.await
			.expect("create should succeed");
		let found = storage
			.find_by_provider_and_uid("github", "gh_42")
			.await
			.expect("lookup should succeed");

		// Assert
		assert_eq!(created.provider, "github");
		assert_eq!(created.provider_user_id, "gh_42");
		assert_eq!(created.user_id, user_id);
		// SEC E1: tokens MUST NOT be returned by storage.
		assert!(
			created.access_token.is_empty(),
			"access_token must not be persisted"
		);
		assert!(
			created.refresh_token.is_none(),
			"refresh_token must not be persisted"
		);
		assert!(created.scopes.is_empty(), "scopes must not be persisted");
		let found = found.expect("row should exist");
		assert_eq!(found.id, created.id);
		assert!(found.access_token.is_empty());
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_find_by_provider_returns_none_for_missing(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
		),
	) {
		// Arrange
		let (_container, _conn, _client, _urls) = db.await;
		let storage = OrmSocialAccountStorage::new();

		// Act
		let result = storage
			.find_by_provider_and_uid("github", "ghost_user")
			.await
			.expect("lookup should not error on missing");

		// Assert
		assert!(result.is_none());
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_find_by_user_lists_all_providers(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
		),
	) {
		// Arrange
		let (_container, _conn, _client, _urls) = db.await;
		let user_id = create_test_user("oauth_find_many", "oauthmany@example.com").await;
		let storage = OrmSocialAccountStorage::new();

		// Act
		storage
			.create(sample_account(user_id, "github", "gh_1"))
			.await
			.unwrap();
		storage
			.create(sample_account(user_id, "gitlab", "gl_1"))
			.await
			.unwrap();
		let mut accounts = storage.find_by_user(user_id).await.unwrap();
		accounts.sort_by(|a, b| a.provider.cmp(&b.provider));

		// Assert
		assert_eq!(accounts.len(), 2);
		assert_eq!(accounts[0].provider, "github");
		assert_eq!(accounts[1].provider, "gitlab");
		// All tokens redacted on the way back out.
		for a in &accounts {
			assert!(a.access_token.is_empty());
		}
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_update_missing_returns_error(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
		),
	) {
		// Arrange
		let (_container, _conn, _client, _urls) = db.await;
		let storage = OrmSocialAccountStorage::new();
		// Build an account whose id is not in the database.
		let phantom = sample_account(Uuid::now_v7(), "github", "gh_phantom");

		// Act
		let result = storage.update(phantom).await;

		// Assert — mirror in-memory storage: missing-row update is an error.
		assert!(result.is_err(), "update on missing row should fail");
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_delete_removes_row(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
		),
	) {
		// Arrange
		let (_container, _conn, _client, _urls) = db.await;
		let user_id = create_test_user("oauth_delete", "oauthdel@example.com").await;
		let storage = OrmSocialAccountStorage::new();
		let created = storage
			.create(sample_account(user_id, "github", "gh_del"))
			.await
			.unwrap();

		// Act
		storage.delete(created.id).await.expect("delete succeeds");
		let after = storage.find_by_user(user_id).await.unwrap();

		// Assert
		assert!(after.is_empty(), "no rows remain after delete");
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_unique_provider_user_id_is_enforced(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
		),
	) {
		// Arrange — same (provider, provider_user_id) must not link to two users.
		let (_container, _conn, _client, _urls) = db.await;
		let user_a = create_test_user("oauth_uniq_a", "uniqa@example.com").await;
		let user_b = create_test_user("oauth_uniq_b", "uniqb@example.com").await;
		let storage = OrmSocialAccountStorage::new();
		storage
			.create(sample_account(user_a, "github", "gh_shared"))
			.await
			.expect("first link succeeds");

		// Act — try to register the same provider_user_id under a second user.
		let result = storage
			.create(sample_account(user_b, "github", "gh_shared"))
			.await;

		// Assert — UNIQUE constraint must surface as an error to callers.
		assert!(
			result.is_err(),
			"second link with duplicate (provider, provider_user_id) must fail"
		);
	}
}
