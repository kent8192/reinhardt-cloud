//! Integration tests for credential verification service.

#[cfg(test)]
mod tests {
	use reinhardt::db::orm::{FilterOperator, FilterValue, Model};
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::test::APIClient;
	use reinhardt::test::fixtures::TestServerGuard;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage, api_client_from_url};
	use reinhardt::test::fixtures::{postgres_with_migrations_from_dir, test_server_guard};
	use rstest::*;
	use serde_json::json;
	use serial_test::serial;
	use std::sync::Arc;

	use crate::apps::auth::models::User;
	use crate::apps::auth::services::credentials::verify_credentials;
	use crate::routes;

	#[fixture]
	async fn test_app() -> (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		TestServerGuard,
		APIClient,
	) {
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations");
		let router = routes().into_server();
		let server = test_server_guard(router).await;
		let client = api_client_from_url(&server.url);
		(container, conn, server, client)
	}

	/// Helper: register a user via the API.
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
	}

	/// verify_credentials with correct password returns Ok with matching username.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_verify_credentials_valid_user(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange
		let (_container, _conn, _server, client) = test_app.await;
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
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange
		let (_container, _conn, _server, client) = test_app.await;
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
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange
		let (_container, _conn, _server, _client) = test_app.await;

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
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange — register user then deactivate via ORM
		let (_container, conn, _server, client) = test_app.await;
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
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange
		let (_container, _conn, _server, client) = test_app.await;
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
