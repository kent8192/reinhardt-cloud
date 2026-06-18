//! Integration tests for `ApiTokenAuthMiddleware` bearer -> AuthState resolution.
//!
//! Exercises the pure `resolve_auth_state_for_bearer` helper (the verification
//! core factored out of the middleware) against a real PostgreSQL instance.

#[cfg(test)]
mod tests {
	use reinhardt::UrlReverser;
	use reinhardt::db::orm::Model;
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::test::APIClient;
	use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage};
	use rstest::*;
	use serial_test::serial;
	use std::sync::Arc;

	use crate::apps::auth::middleware::api_token::resolve_auth_state_for_bearer;
	use crate::apps::auth::models::User;
	use crate::apps::auth::services::api_key::generate_api_key;
	use crate::config::test_helpers::build_test_app;

	#[fixture]
	async fn db() -> (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		Arc<UrlReverser>,
	) {
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations");
		let (client, urls) = build_test_app();
		(container, conn, client, urls)
	}

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

	/// A valid bearer token resolves to an authenticated (non-anonymous) state.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_resolve_valid_token_authenticated(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<UrlReverser>,
		),
	) {
		// Arrange
		let (_container, _conn, _client, _urls) = db.await;
		let user = create_test_user("middleware-alice").await;
		let (plaintext, _model) = generate_api_key(user.id, "test".to_string(), None)
			.await
			.expect("generate");

		// Act
		let state = resolve_auth_state_for_bearer(&plaintext).await;

		// Assert
		assert!(
			state.is_authenticated(),
			"valid token must resolve to an authenticated state"
		);
		assert!(!state.is_anonymous());
	}

	/// An unknown / malformed bearer token resolves to an anonymous state.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_resolve_invalid_token_anonymous(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<UrlReverser>,
		),
	) {
		// Arrange — container up so the global ORM is initialized for the
		// lookup path, but no real key is created.
		let (_container, _conn, _client, _urls) = db.await;

		// Act
		let state = resolve_auth_state_for_bearer("rct_bogus-not-a-real-token").await;

		// Assert
		assert!(
			!state.is_authenticated(),
			"unknown token must resolve to an anonymous state"
		);
		assert!(state.is_anonymous());
	}
}
