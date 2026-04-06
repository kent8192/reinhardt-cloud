//! End-to-end tests for auth error paths.

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

	/// Login with a non-existent user should not return 200.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_login_nonexistent_user_returns_401(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange
		let (_container, _conn, _server, client) = test_app.await;
		let login_data = json!({
			"username": "ghost_user",
			"password": "doesnotmatter"
		});

		// Act
		let response = client
			.post("/api/auth/login/", &login_data, "json")
			.await
			.expect("Login request failed");

		// Assert
		assert_ne!(response.status_code(), 200);
		let body: serde_json::Value = response.json().expect("Failed to parse JSON response");
		assert!(
			body.get("error").is_some() || body.get("detail").is_some(),
			"Error response should contain 'error' or 'detail' field, got: {body}"
		);
	}

	/// Login with empty JSON body should return 400 (validation error).
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_login_empty_body_returns_422(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange
		let (_container, _conn, _server, client) = test_app.await;
		let empty_body = json!({});

		// Act
		let response = client
			.post("/api/auth/login/", &empty_body, "json")
			.await
			.expect("Login request failed");

		// Assert
		assert_eq!(response.status_code(), 400);
	}

	/// Register with empty JSON body should return 400 (validation error).
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_register_empty_body_returns_422(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange
		let (_container, _conn, _server, client) = test_app.await;
		let empty_body = json!({});

		// Act
		let response = client
			.post("/api/auth/register/", &empty_body, "json")
			.await
			.expect("Register request failed");

		// Assert
		assert_eq!(response.status_code(), 400);
	}

	/// Registering with a duplicate username (different email) should return 409.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_register_duplicate_username_returns_conflict(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange -- register first user
		let (_container, _conn, _server, client) = test_app.await;
		let first_user = json!({
			"username": "dupeuser",
			"email": "dupe1@example.com",
			"password": "securepassword"
		});
		let first_response = client
			.post("/api/auth/register/", &first_user, "json")
			.await
			.expect("First register request failed");
		assert_eq!(first_response.status_code(), 201);

		// Act -- register second user with same username but different email
		let second_user = json!({
			"username": "dupeuser",
			"email": "dupe2@example.com",
			"password": "securepassword"
		});
		let response = client
			.post("/api/auth/register/", &second_user, "json")
			.await
			.expect("Second register request failed");

		// Assert
		assert_eq!(response.status_code(), 409);
		let body: serde_json::Value = response.json().expect("Failed to parse JSON response");
		assert_eq!(body["detail"], "Username already exists");
	}

	/// Login with an inactive user should not return 200.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_login_inactive_user_returns_401(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange -- register a user
		let (_container, conn, _server, client) = test_app.await;
		let register_data = json!({
			"username": "inactiveuser",
			"email": "inactive@example.com",
			"password": "securepassword"
		});
		let reg_response = client
			.post("/api/auth/register/", &register_data, "json")
			.await
			.expect("Register request failed");
		assert_eq!(reg_response.status_code(), 201);

		// Deactivate the user via ORM
		let mut user = User::objects()
			.filter(
				User::field_username(),
				FilterOperator::Eq,
				FilterValue::String("inactiveuser".to_string()),
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

		// Act -- login with deactivated user
		let login_data = json!({
			"username": "inactiveuser",
			"password": "securepassword"
		});
		let response = client
			.post("/api/auth/login/", &login_data, "json")
			.await
			.expect("Login request failed");

		// Assert
		assert_ne!(
			response.status_code(),
			200,
			"Inactive user login should not return 200"
		);
	}
}
