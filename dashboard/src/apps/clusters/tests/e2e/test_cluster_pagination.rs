//! Pagination tests for clusters API.

#[cfg(test)]
mod tests {
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::test::APIClient;
	use reinhardt::test::fixtures::TestServerGuard;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage, api_client_from_url};
	use reinhardt::test::fixtures::{postgres_with_migrations_from_dir, test_server_guard};
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
		unsafe {
			std::env::set_var(
				"REINHARDT_CLOUD_JWT_SECRET",
				"test-secret-minimum-32-bytes-long!!",
			);
		}
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations");
		let router = routes().into_server();
		let server = test_server_guard(router).await;
		let client = api_client_from_url(&server.url);
		(container, conn, server, client)
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

	/// Helper: create N clusters with sequential names.
	async fn create_clusters(client: &APIClient, count: usize) {
		for i in 0..count {
			let data = json!({
				"name": format!("cluster-{}", i),
				"api_url": "https://k8s.example.com:6443"
			});
			let resp = client
				.post("/api/clusters/", &data, "json")
				.await
				.expect("Create cluster request failed");
			assert_eq!(resp.status_code(), 201);
		}
	}

	/// Default pagination returns all items when count is small.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_clusters_default_pagination(
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
		create_clusters(&client, 3).await;

		// Act
		let response = client
			.get("/api/clusters/")
			.await
			.expect("List clusters request failed");

		// Assert
		assert_eq!(response.status_code(), 200);
		let body: serde_json::Value = response.json().expect("Failed to parse JSON");
		assert_eq!(body["total"], 3);
		let items = body["items"].as_array().expect("items should be an array");
		assert_eq!(items.len(), 3);
		assert_eq!(body["page"], 1);
	}

	/// Custom page_size limits the number of returned items.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_clusters_custom_page_size(
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
		create_clusters(&client, 5).await;

		// Act
		let response = client
			.get("/api/clusters/?page_size=2")
			.await
			.expect("List clusters request failed");

		// Assert
		assert_eq!(response.status_code(), 200);
		let body: serde_json::Value = response.json().expect("Failed to parse JSON");
		assert_eq!(body["total"], 5);
		let items = body["items"].as_array().expect("items should be an array");
		assert_eq!(items.len(), 2);
	}

	/// Requesting a page beyond total results returns empty items.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_clusters_page_beyond_total(
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
		create_clusters(&client, 2).await;

		// Act
		let response = client
			.get("/api/clusters/?page=5&page_size=2")
			.await
			.expect("List clusters request failed");

		// Assert
		assert_eq!(response.status_code(), 200);
		let body: serde_json::Value = response.json().expect("Failed to parse JSON");
		let items = body["items"].as_array().expect("items should be an array");
		assert_eq!(items.len(), 0);
	}

	/// page_size is capped at 100 even if a larger value is requested.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_clusters_page_size_capped_at_100(
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
			.get("/api/clusters/?page_size=500")
			.await
			.expect("List clusters request failed");

		// Assert
		assert_eq!(response.status_code(), 200);
		let body: serde_json::Value = response.json().expect("Failed to parse JSON");
		let page_size = body["page_size"]
			.as_i64()
			.expect("page_size should be a number");
		assert!(
			page_size <= 100,
			"page_size should be capped at 100 but got {}",
			page_size
		);
	}
}
