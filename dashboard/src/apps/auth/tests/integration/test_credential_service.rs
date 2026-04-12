//! Integration tests for credential verification service.

#[cfg(test)]
mod tests {
	use reinhardt::db::orm::{FilterOperator, FilterValue, Model};
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::test::APIClient;
	use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage};
	use rstest::*;
	use serde_json::json;
	use serial_test::serial;
	use std::sync::Arc;

	use crate::apps::auth::models::User;
	use crate::apps::auth::services::credentials::verify_credentials;
	use crate::config::test_helpers::{TestUrls, test_app};

	#[fixture]
	async fn db(
		test_app: (APIClient, TestUrls),
	) -> (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		TestUrls,
	) {
		let (client, urls) = test_app;
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations");
		(container, conn, client, urls)
	}

	/// Helper: register a user via the API and activate via ORM.
	async fn register_user(client: &APIClient, username: &str, email: &str, password: &str) {
		let data = json!({
			"username": username,
			"email": email,
			"password": password,
		});
		let response = client
			.post("/api/auth/register/", &data, "json")
			.await
			.expect("Register request failed");
		assert_eq!(response.status_code(), 201, "Registration should succeed");

		// Activate user via ORM (registration creates inactive user)
		let mut user = User::objects()
			.filter(
				User::field_username(),
				FilterOperator::Eq,
				FilterValue::String(username.to_string()),
			)
			.first()
			.await
			.expect("Failed to query user")
			.expect("User not found");
		user.is_active = true;
		User::objects()
			.update(&user)
			.await
			.expect("Failed to activate user");
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
			TestUrls,
		),
	) {
		// Arrange
		let (_container, _conn, client, _urls) = db.await;
		register_user(&client, "creduser", "cred@example.com", "securepassword").await;

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
			TestUrls,
		),
	) {
		// Arrange
		let (_container, _conn, client, _urls) = db.await;
		register_user(
			&client,
			"wrongpwuser",
			"wrongpw@example.com",
			"correctpassword",
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
			TestUrls,
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
			TestUrls,
		),
	) {
		// Arrange — register user then deactivate via ORM
		let (_container, conn, client, _urls) = db.await;
		register_user(
			&client,
			"inactivecred",
			"inactivecred@example.com",
			"securepassword",
		)
		.await;

		let mut user = User::objects()
			.filter(
				User::field_username(),
				FilterOperator::Eq,
				FilterValue::String("inactivecred".to_string()),
			)
			.first_with_db(&conn)
			.await
			.expect("Failed to query user")
			.expect("User should exist after registration");
		user.is_active = false;
		User::objects()
			.update_with_conn(&conn, &user)
			.await
			.expect("Failed to deactivate user");

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
			TestUrls,
		),
	) {
		// Arrange
		let (_container, _conn, client, _urls) = db.await;
		register_user(
			&client,
			"trimcreduser",
			"trimcred@example.com",
			"securepassword",
		)
		.await;

		// Act — pass username with surrounding whitespace
		let result = verify_credentials(" trimcreduser ", "securepassword").await;

		// Assert
		let user = result.expect("verify_credentials should trim whitespace from username");
		assert_eq!(user.username, "trimcreduser");
	}
}
