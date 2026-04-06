//! Pagination tests for deployments API.

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

	/// Helper: register a user and return the JWT token.
	async fn register_and_get_token(client: &APIClient) -> String {
		let data = json!({
			"username": "testuser",
			"email": "test@example.com",
			"password": "securepassword123"
		});
		let resp = client
			.post("/api/auth/register/", &data, "json")
			.await
			.expect("Register request failed");
		assert_eq!(resp.status_code(), 201);
		let body: serde_json::Value = resp.json().expect("Failed to parse register response");
		body["token"].as_str().unwrap().to_string()
	}

	/// Helper: set Authorization header on client.
	async fn authenticate_client(client: &APIClient, token: &str) {
		client
			.set_header("Authorization", format!("Bearer {token}"))
			.await
			.expect("Failed to set Authorization header");
	}

	/// Helper: create a cluster and return its id.
	async fn create_cluster(client: &APIClient) -> i64 {
		let data = json!({
			"name": "test-cluster",
			"api_url": "https://k8s.example.com:6443"
		});
		let resp = client
			.post("/api/clusters/", &data, "json")
			.await
			.expect("Create cluster failed");
		assert_eq!(resp.status_code(), 201);
		let body: serde_json::Value = resp.json().expect("Failed to parse cluster response");
		body["id"].as_i64().expect("cluster id")
	}

	/// Helper: create a deployment with a unique app_name.
	async fn create_deployment(client: &APIClient, cluster_id: i64, suffix: &str) {
		let data = json!({
			"app_name": format!("app-{suffix}"),
			"cluster_id": cluster_id,
			"image": "nginx:latest"
		});
		let resp = client
			.post("/api/deployments/", &data, "json")
			.await
			.expect("Create deployment failed");
		assert_eq!(resp.status_code(), 201);
	}

	/// Default pagination returns all items when count is small.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_deployments_default_pagination(
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
		let cluster_id = create_cluster(&client).await;
		for i in 1..=3 {
			create_deployment(&client, cluster_id, &i.to_string()).await;
		}

		// Act
		let response = client
			.get("/api/deployments/")
			.await
			.expect("List deployments failed");

		// Assert
		assert_eq!(response.status_code(), 200);
		let body: serde_json::Value = response.json().expect("Failed to parse response");
		assert_eq!(body["total"], 3);
		let items = body["items"].as_array().expect("items should be array");
		assert_eq!(items.len(), 3);
	}

	/// Page 2 with page_size=2 returns the remaining item.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_deployments_page_2(
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
		let cluster_id = create_cluster(&client).await;
		for i in 1..=3 {
			create_deployment(&client, cluster_id, &i.to_string()).await;
		}

		// Act
		let response = client
			.get("/api/deployments/?page=2&page_size=2")
			.await
			.expect("List deployments page 2 failed");

		// Assert
		assert_eq!(response.status_code(), 200);
		let body: serde_json::Value = response.json().expect("Failed to parse response");
		let items = body["items"].as_array().expect("items should be array");
		assert_eq!(items.len(), 1);
		assert_eq!(body["total"], 3);
	}

	/// Requesting a page beyond available data returns empty items.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_deployments_empty_page(
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
		let cluster_id = create_cluster(&client).await;
		for i in 1..=3 {
			create_deployment(&client, cluster_id, &i.to_string()).await;
		}

		// Act
		let response = client
			.get("/api/deployments/?page=99")
			.await
			.expect("List deployments page 99 failed");

		// Assert
		assert_eq!(response.status_code(), 200);
		let body: serde_json::Value = response.json().expect("Failed to parse response");
		let items = body["items"].as_array().expect("items should be array");
		assert_eq!(items.len(), 0);
	}
}
