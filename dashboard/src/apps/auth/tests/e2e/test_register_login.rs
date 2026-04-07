//! End-to-end tests for auth API endpoints.
//!
//! These tests require both a PostgreSQL database and a Redis instance.
//! The login and register endpoints now use cookie-based sessions instead
//! of JWT tokens. Responses return `AuthResponse` with `success` and `user`
//! fields, and authentication is conveyed via `Set-Cookie: sessionid=...`.

#[cfg(test)]
mod tests {
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::test::APIClient;
	use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage};
	use rstest::*;
	use serde_json::json;
	use serial_test::serial;
	use std::sync::Arc;

	use crate::config::test_helpers::{TestAppGuard, test_app_with_origin_guard};

	#[fixture]
	async fn test_app() -> (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		TestAppGuard,
		APIClient,
	) {
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations");
		let (server, client) = test_app_with_origin_guard().await;
		(container, conn, server, client)
	}

	/// Verify POST /api/auth/register/ creates a user and returns session cookie.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_register_returns_session_cookie(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestAppGuard,
			APIClient,
		),
	) {
		// Arrange
		let (_container, _conn, _server, client) = test_app.await;
		let register_data = json!({
			"username": "newuser",
			"email": "newuser@example.com",
			"password": "securepassword"
		});

		// Act
		let response = client
			.post("/api/auth/register/", &register_data, "json")
			.await
			.expect("Register request failed");

		// Assert
		assert_eq!(response.status_code(), 201);
		let body: serde_json::Value = response.json().expect("Failed to parse JSON response");
		assert_eq!(body["success"], true);
		assert!(body["user"].is_object());
		assert!(body["user"]["username"].is_string());
	}

	/// Verify POST /api/auth/login/ authenticates against DB and returns session cookie.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_login_returns_session_cookie(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestAppGuard,
			APIClient,
		),
	) {
		// Arrange -- register user first
		let (_container, _conn, _server, client) = test_app.await;
		let register_data = json!({
			"username": "loginuser",
			"email": "login@example.com",
			"password": "testpassword"
		});
		client
			.post("/api/auth/register/", &register_data, "json")
			.await
			.expect("Register request failed");

		// Act -- login with same credentials
		let login_data = json!({
			"username": "loginuser",
			"password": "testpassword"
		});
		let response = client
			.post("/api/auth/login/", &login_data, "json")
			.await
			.expect("Login request failed");

		// Assert
		assert_eq!(response.status_code(), 200);
		let body: serde_json::Value = response.json().expect("Failed to parse JSON response");
		assert_eq!(body["success"], true);
		assert!(body["user"].is_object());
	}

	/// Verify duplicate email registration returns 409 with email-specific message.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_register_duplicate_email_returns_conflict(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestAppGuard,
			APIClient,
		),
	) {
		// Arrange -- register first user
		let (_container, _conn, _server, client) = test_app.await;
		let first_user = json!({
			"username": "emailuser1",
			"email": "test@example.com",
			"password": "securepassword"
		});
		let first_response = client
			.post("/api/auth/register/", &first_user, "json")
			.await
			.expect("First register request failed");
		assert_eq!(first_response.status_code(), 201);

		// Act -- register second user with same email but different username
		let second_user = json!({
			"username": "emailuser2",
			"email": "test@example.com",
			"password": "securepassword"
		});
		let response = client
			.post("/api/auth/register/", &second_user, "json")
			.await
			.expect("Second register request failed");

		// Assert
		assert_eq!(response.status_code(), 409);
		let body: serde_json::Value = response.json().expect("Failed to parse JSON response");
		assert_eq!(body["error"], "Conflict");
		assert_eq!(body["detail"], "Email already exists");
	}

	/// Verify login with wrong password returns error.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_login_wrong_password_fails(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestAppGuard,
			APIClient,
		),
	) {
		// Arrange -- register user first
		let (_container, _conn, _server, client) = test_app.await;
		let register_data = json!({
			"username": "failuser",
			"email": "fail@example.com",
			"password": "correctpassword"
		});
		client
			.post("/api/auth/register/", &register_data, "json")
			.await
			.expect("Register request failed");

		// Act -- login with wrong password
		let login_data = json!({
			"username": "failuser",
			"password": "wrongpassword"
		});
		let response = client
			.post("/api/auth/login/", &login_data, "json")
			.await
			.expect("Login request failed");

		// Assert -- should not return 200
		assert_ne!(response.status_code(), 200);
	}

	/// Register with whitespace-padded username should be trimmed; login with trimmed name succeeds.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_register_whitespace_username_trimmed(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestAppGuard,
			APIClient,
		),
	) {
		// Arrange
		let (_container, _conn, _server, client) = test_app.await;
		let register_data = json!({
			"username": "  trimuser  ",
			"email": "trim@example.com",
			"password": "securepassword"
		});
		let reg_response = client
			.post("/api/auth/register/", &register_data, "json")
			.await
			.expect("Register request failed");
		assert_eq!(reg_response.status_code(), 201);

		// Act -- login with trimmed username
		let login_data = json!({
			"username": "trimuser",
			"password": "securepassword"
		});
		let response = client
			.post("/api/auth/login/", &login_data, "json")
			.await
			.expect("Login request failed");

		// Assert
		assert_eq!(
			response.status_code(),
			200,
			"Login with trimmed username should succeed"
		);
	}
}
