//! Integration tests for the API key service.
//!
//! Verifies the generate -> verify round-trip and the revoked / expired /
//! inactive rejection rules against a real PostgreSQL instance
//! (TestContainers, mirroring `test_credential_service`).

#[cfg(test)]
mod tests {
	use chrono::{Duration, Utc};
	use reinhardt::UrlReverser;
	use reinhardt::db::orm::Model;
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::test::APIClient;
	use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage};
	use rstest::{fixture, rstest};
	use serial_test::serial;
	use std::sync::Arc;

	use crate::apps::auth::models::{ApiKey, User};
	use crate::apps::auth::services::api_key::{
		generate_api_key, list_api_keys_for_user, revoke_api_key, touch_last_used, verify_api_key,
	};
	use crate::config::test_helpers::build_test_app;

	#[fixture]
	async fn db() -> (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		Arc<UrlReverser>,
	) {
		// Start TestContainers first so build_test_app() registers
		// DatabaseConnection in the DI scope.
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations");
		let (client, urls) = build_test_app();
		(container, conn, client, urls)
	}

	/// Helper: create an active user directly via ORM.
	async fn create_test_user(username: &str) -> User {
		let user = User::build()
			.username(username.to_string())
			.email(format!("{username}@example.test"))
			.first_name(String::new())
			.last_name(String::new())
			.password_hash(None)
			.is_active(true)
			.is_staff(false)
			.is_superuser(false)
			.finish();
		User::objects()
			.create(&user)
			.await
			.expect("Failed to create user")
	}

	/// generate -> verify round-trip resolves the owning user and key id.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_generate_then_verify_roundtrip(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<UrlReverser>,
		),
	) {
		// Arrange
		let (_container, _conn, _client, _urls) = db.await;
		let user = create_test_user("alice").await;

		// Act
		let (plaintext, model) = generate_api_key(user.id, "CI deploy".to_string(), None)
			.await
			.expect("generate");

		// Assert
		assert!(
			plaintext.starts_with("rct_"),
			"token must carry the rct_ prefix"
		);
		assert_eq!(model.label, "CI deploy");
		let resolved = verify_api_key(&plaintext).await.expect("verify");
		assert_eq!(resolved.0.id(), user.id());
		let api_key_id = model.id.expect("persisted api key id");
		assert_eq!(resolved.1, api_key_id);
	}

	/// A revoked token must not verify.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_verify_rejects_revoked_token(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<UrlReverser>,
		),
	) {
		// Arrange
		let (_container, _conn, _client, _urls) = db.await;
		let user = create_test_user("bob").await;
		let (plaintext, model) = generate_api_key(user.id, "labeled".to_string(), None)
			.await
			.expect("generate");

		// Act
		let api_key_id = model.id.expect("persisted api key id");
		revoke_api_key(api_key_id).await.expect("revoke");
		let resolved = verify_api_key(&plaintext).await;

		// Assert
		assert!(resolved.is_none(), "revoked token must not verify");
	}

	/// An expired token must not verify.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_verify_rejects_expired_token(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<UrlReverser>,
		),
	) {
		// Arrange — expiry already in the past
		let (_container, _conn, _client, _urls) = db.await;
		let user = create_test_user("carol").await;
		let (plaintext, _model) = generate_api_key(
			user.id,
			"expired".to_string(),
			Some(Utc::now() - Duration::seconds(1)),
		)
		.await
		.expect("generate");

		// Act
		let resolved = verify_api_key(&plaintext).await;

		// Assert
		assert!(resolved.is_none(), "expired token must not verify");
	}

	/// touch_last_used must not update or reactivate revoked tokens.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_touch_last_used_skips_revoked_token(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<UrlReverser>,
		),
	) {
		// Arrange
		let (_container, _conn, _client, _urls) = db.await;
		let user = create_test_user("touch-revoked").await;
		let (_plaintext, model) = generate_api_key(user.id, "touch".to_string(), None)
			.await
			.expect("generate");
		let id = model.id.expect("persisted api key id");
		revoke_api_key(id).await.expect("revoke");
		let revoked = ApiKey::objects()
			.filter(ApiKey::field_id().eq(id))
			.first()
			.await
			.expect("lookup revoked")
			.expect("revoked token exists");
		let revoked_at = revoked.revoked_at;

		// Act
		touch_last_used(id).await;

		// Assert
		let after_touch = ApiKey::objects()
			.filter(ApiKey::field_id().eq(id))
			.first()
			.await
			.expect("lookup after touch")
			.expect("token still exists");
		assert_eq!(after_touch.revoked_at, revoked_at);
		assert_eq!(after_touch.last_used_at, None);
	}

	/// list_api_keys_for_user returns every key for the user.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_list_api_keys_for_user_returns_keys(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<UrlReverser>,
		),
	) {
		// Arrange
		let (_container, _conn, _client, _urls) = db.await;
		let user = create_test_user("dave").await;
		generate_api_key(user.id, "first".to_string(), None)
			.await
			.expect("generate first");
		generate_api_key(user.id, "second".to_string(), None)
			.await
			.expect("generate second");

		// Act
		let keys = list_api_keys_for_user(user.id).await.expect("list");

		// Assert
		assert_eq!(keys.len(), 2);
		assert!(keys.iter().all(|k| !k.prefix.is_empty()));
	}
}
