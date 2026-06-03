//! Integration tests for credential verification service.

#[cfg(test)]
mod tests {
	use reinhardt::BaseUser;
	use reinhardt::db::orm::Model;
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::test::APIClient;
	use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage};
	use rstest::*;
	use serial_test::serial;
	use std::sync::Arc;

	use crate::apps::auth::models::User;
	use crate::apps::auth::services::credentials::verify_credentials;
	use reinhardt::UrlReverser;

	#[fixture]
	async fn db() -> (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		Arc<UrlReverser>,
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

	/// Helper: create a user directly via ORM (bypasses register endpoint and email).
	async fn create_test_user(username: &str, email: &str, password: &str, active: bool) {
		let mut user = User::build()
			.username(username.to_string())
			.email(email.to_lowercase())
			.first_name(String::new())
			.last_name(String::new())
			.password_hash(None)
			.is_active(active)
			.is_staff(false)
			.is_superuser(false)
			.finish();
		user.set_password(password)
			.expect("Password hashing failed");
		User::objects()
			.create(&user)
			.await
			.expect("Failed to create user");
	}

	/// verify_credentials with correct password returns Ok with matching username.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_verify_credentials_valid_user(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<UrlReverser>,
		),
	) {
		// Arrange
		let (_container, _conn, _client, _urls) = db.await;
		create_test_user("creduser", "cred@example.com", "securepassword", true).await;

		// Act
		let result = verify_credentials("creduser", "securepassword").await;

		// Assert
		let user = result.expect("verify_credentials should succeed for valid user");
		assert_eq!(user.username, "creduser");
	}

	/// verify_credentials with wrong password returns Err.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_verify_credentials_wrong_password(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<UrlReverser>,
		),
	) {
		// Arrange
		let (_container, _conn, _client, _urls) = db.await;
		create_test_user(
			"wrongpwuser",
			"wrongpw@example.com",
			"correctpassword",
			true,
		)
		.await;

		// Act
		let result = verify_credentials("wrongpwuser", "incorrectpassword").await;

		// Assert
		assert!(
			result.is_err(),
			"verify_credentials should fail with wrong password"
		);
	}

	/// verify_credentials for non-existent user returns Err.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_verify_credentials_nonexistent_user(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<UrlReverser>,
		),
	) {
		// Arrange
		let (_container, _conn, _client, _urls) = db.await;

		// Act
		let result = verify_credentials("ghost", "pw").await;

		// Assert
		assert!(
			result.is_err(),
			"verify_credentials should fail for non-existent user"
		);
	}

	/// verify_credentials for inactive user returns Err.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_verify_credentials_inactive_user(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<UrlReverser>,
		),
	) {
		// Arrange — create inactive user via ORM
		let (_container, _conn, _client, _urls) = db.await;
		create_test_user(
			"inactivecred",
			"inactivecred@example.com",
			"securepassword",
			false,
		)
		.await;

		// Act
		let result = verify_credentials("inactivecred", "securepassword").await;

		// Assert
		assert!(
			result.is_err(),
			"verify_credentials should fail for inactive user"
		);
	}

	/// verify_credentials should trim whitespace from username.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_verify_credentials_whitespace_trimmed(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<UrlReverser>,
		),
	) {
		// Arrange
		let (_container, _conn, _client, _urls) = db.await;
		create_test_user(
			"trimcreduser",
			"trimcred@example.com",
			"securepassword",
			true,
		)
		.await;

		// Act — pass username with surrounding whitespace
		let result = verify_credentials(" trimcreduser ", "securepassword").await;

		// Assert
		let user = result.expect("verify_credentials should trim whitespace from username");
		assert_eq!(user.username, "trimcreduser");
	}
}
