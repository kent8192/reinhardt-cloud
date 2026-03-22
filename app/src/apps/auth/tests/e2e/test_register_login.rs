//! End-to-end tests for auth API endpoints.

#[cfg(test)]
mod tests {
	use reinhardt::db::migrations::executor::DatabaseMigrationExecutor;
	use reinhardt::db::migrations::{FilesystemSource, MigrationSource};
	use reinhardt::db::orm::reinitialize_database;
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::test::APIClient;
	use reinhardt::test::fixtures::TestServerGuard;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage, api_client_from_url};
	use reinhardt::test::fixtures::{postgres_container, test_server_guard};
	use rstest::*;
	use serde_json::json;
	use serial_test::serial;
	use std::sync::Arc;

	use crate::routes;

	#[fixture]
	async fn test_app() -> (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		TestServerGuard,
		APIClient,
	) {
		// Arrange — set required environment variable for JWT secret
		unsafe {
			std::env::set_var(
				"REINHARDT_CLOUD_JWT_SECRET",
				"test-secret-minimum-32-bytes-long!!",
			);
		}
		// Workaround for kent8192/reinhardt-web#2845 (tracked in reinhardt-cloud#114)
		// Remove this workaround when the upstream issue is resolved.
		//
		// Ideal implementation (without workaround):
		//   let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		//   let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
		//       .await
		//       .expect("Failed to start PostgreSQL with migrations");
		let (container, _pool, _port, database_url) = postgres_container().await;
		let conn = DatabaseConnection::connect(&database_url)
			.await
			.expect("Failed to connect to PostgreSQL");
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let source = FilesystemSource::new(migrations_dir);
		let migrations = source
			.all_migrations()
			.await
			.expect("Failed to load migrations");
		if !migrations.is_empty() {
			let mut executor = DatabaseMigrationExecutor::new(conn.inner().clone());
			executor
				.apply_migrations(&migrations)
				.await
				.expect("Failed to apply migrations");
		}
		reinitialize_database(&database_url)
			.await
			.expect("Failed to initialize global database state");
		let router = routes().into_server();
		let server = test_server_guard(router).await;
		let client = api_client_from_url(&server.url);
		(container, Arc::new(conn), server, client)
	}

	/// Verify POST /api/auth/register/ creates a user and returns JWT.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_register_returns_jwt_token(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
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
		assert_eq!(body["token_type"], "Bearer");
		assert!(body["token"].is_string());
		assert!(!body["token"].as_str().unwrap().is_empty());
	}

	/// Verify POST /api/auth/login/ authenticates against DB and returns JWT.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_login_returns_jwt_token(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange — register user first
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

		// Act — login with same credentials
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
		assert_eq!(body["token_type"], "Bearer");
		assert!(body["token"].is_string());
		assert!(!body["token"].as_str().unwrap().is_empty());
	}

	/// Verify duplicate email registration returns 409 with email-specific message.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_register_duplicate_email_returns_conflict(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange — register first user
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

		// Act — register second user with same email but different username
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
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange — register user first
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

		// Act — login with wrong password
		let login_data = json!({
			"username": "failuser",
			"password": "wrongpassword"
		});
		let response = client
			.post("/api/auth/login/", &login_data, "json")
			.await
			.expect("Login request failed");

		// Assert — should not return 200
		assert_ne!(response.status_code(), 200);
	}
}
