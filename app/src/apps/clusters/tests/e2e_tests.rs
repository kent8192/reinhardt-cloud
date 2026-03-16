//! End-to-end tests for clusters API endpoints.

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
		let (container, _pool, _port, database_url) = postgres_container().await;
		let conn = DatabaseConnection::connect(&database_url)
			.await
			.expect("Failed to connect to PostgreSQL");
		// Workaround: Use FilesystemSource directly instead of postgres_with_all_migrations
		// fixture, which relies on global_registry() requiring collect_migrations! registration.
		// See: https://github.com/kent8192/reinhardt-web/issues/2415
		let source = FilesystemSource::new("migrations");
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

	/// Verify unauthenticated GET /api/clusters/ returns 401.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_unauthenticated_clusters_returns_401(
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
			.get("/api/clusters/")
			.await
			.expect("List clusters request failed");

		// Assert
		assert_eq!(response.status_code(), 401);
	}

	/// Verify GET /api/clusters/ returns empty list when authenticated.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_list_clusters_empty(
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
			.get("/api/clusters/")
			.await
			.expect("List clusters request failed");

		// Assert
		assert_eq!(response.status_code(), 200);
		let body: Vec<serde_json::Value> = response.json().expect("Failed to parse JSON response");
		assert_eq!(body.len(), 0);
	}

	/// Verify POST /api/clusters/ creates a cluster, then GET returns it.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_create_cluster_persists(
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

		let cluster_data = json!({
			"name": "production-cluster",
			"api_url": "https://k8s.example.com:6443"
		});

		// Act — create cluster
		let create_response = client
			.post("/api/clusters/", &cluster_data, "json")
			.await
			.expect("Create cluster request failed");

		// Assert — creation response
		assert_eq!(create_response.status_code(), 201);
		let created: serde_json::Value = create_response
			.json()
			.expect("Failed to parse create response");
		assert_eq!(created["name"], "production-cluster");
		assert_eq!(created["api_url"], "https://k8s.example.com:6443");
		assert_eq!(created["is_active"], true);
		assert!(created["id"].is_number());

		// Act — list clusters to verify persistence
		let list_response = client
			.get("/api/clusters/")
			.await
			.expect("List clusters request failed");

		// Assert — cluster appears in list
		assert_eq!(list_response.status_code(), 200);
		let clusters: Vec<serde_json::Value> =
			list_response.json().expect("Failed to parse list response");
		assert_eq!(clusters.len(), 1);
		assert_eq!(clusters[0]["name"], "production-cluster");
		assert_eq!(clusters[0]["api_url"], "https://k8s.example.com:6443");
	}
}
