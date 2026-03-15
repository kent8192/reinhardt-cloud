//! End-to-end tests for deployments API endpoints.

#[cfg(test)]
mod tests {
	use reinhardt::db::migrations::MigrationProvider;
	use reinhardt::db::migrations::executor::DatabaseMigrationExecutor;
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

	use crate::{NuagesMigrations, routes};

	#[fixture]
	async fn test_app() -> (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		TestServerGuard,
		APIClient,
	) {
		let (container, _pool, _port, database_url) = postgres_container().await;
		let conn = DatabaseConnection::connect(&database_url)
			.await
			.expect("Failed to connect to PostgreSQL");
		let migrations = NuagesMigrations::migrations();
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

	/// Helper: register a test user and return the JWT token.
	async fn register_and_get_token(client: &APIClient) -> String {
		let register_data = json!({
			"username": "testuser",
			"email": "test@example.com",
			"password": "securepassword123"
		});
		let resp = client
			.post("/api/auth/register/", &register_data, "json")
			.await
			.expect("Register request failed");
		assert_eq!(resp.status_code(), 201);
		let body: serde_json::Value = resp.json().expect("Failed to parse JSON response");
		body["token"].as_str().unwrap().to_string()
	}

	/// Helper: set Authorization header on client.
	async fn authenticate_client(client: &APIClient, token: &str) {
		client
			.set_header("Authorization", format!("Bearer {token}"))
			.await
			.expect("Failed to set Authorization header");
	}

	/// Verify unauthenticated GET /api/deployments/ returns 401.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_unauthenticated_deployments_returns_401(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange
		let (_container, _conn, _server, client) = test_app.await;

		// Act
		let response = client
			.get("/api/deployments/")
			.await
			.expect("List deployments request failed");

		// Assert
		assert_eq!(response.status_code(), 401);
	}

	/// Verify GET /api/deployments/ returns empty list when authenticated.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_list_deployments_empty(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange
		let (_container, _conn, _server, client) = test_app.await;
		let token = register_and_get_token(&client).await;
		authenticate_client(&client, &token).await;

		// Act
		let response = client
			.get("/api/deployments/")
			.await
			.expect("List deployments request failed");

		// Assert
		assert_eq!(response.status_code(), 200);
		let body: Vec<serde_json::Value> = response.json().expect("Failed to parse JSON response");
		assert_eq!(body.len(), 0);
	}
}
