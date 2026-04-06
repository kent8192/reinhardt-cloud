//! Cross-user isolation tests for clusters API.

#[cfg(test)]
mod tests {
	use reinhardt::JwtAuth;
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::test::APIClient;
	use reinhardt::test::fixtures::TestServerGuard;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage, api_client_from_url};
	use reinhardt::test::fixtures::{postgres_with_migrations_from_dir, test_server_guard};
	use rstest::*;
	use serde_json::json;
	use serial_test::serial;
	use std::sync::Arc;
	use uuid::Uuid;

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
	async fn register_user(client: &APIClient, username: &str, email: &str) -> String {
		let data = json!({
			"username": username,
			"email": email,
			"password": "securepassword123"
		});
		let resp = client
			.post("/api/auth/register/", &data, "json")
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

	/// Helper: create a cluster and return its response body.
	async fn create_cluster(client: &APIClient, name: &str) -> serde_json::Value {
		let data = json!({
			"name": name,
			"api_url": "https://k8s.example.com:6443"
		});
		let resp = client
			.post("/api/clusters/", &data, "json")
			.await
			.expect("Create cluster request failed");
		assert_eq!(resp.status_code(), 201);
		resp.json().expect("Failed to parse create response")
	}

	/// UserA creates a cluster; UserB lists clusters and sees nothing.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_user_cannot_see_other_users_clusters(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange
		let (_container, _conn, _server, client) = test_app.await;

		let token_a = register_user(&client, "user_a", "a@example.com").await;
		authenticate_client(&client, &token_a).await;
		create_cluster(&client, "cluster-a").await;

		let token_b = register_user(&client, "user_b", "b@example.com").await;
		authenticate_client(&client, &token_b).await;

		// Act
		let response = client
			.get("/api/clusters/")
			.await
			.expect("List clusters request failed");

		// Assert
		assert_eq!(response.status_code(), 200);
		let body: serde_json::Value = response.json().expect("Failed to parse JSON response");
		assert_eq!(body["items"], json!([]));
		assert_eq!(body["total"], 0);
	}

	/// UserA creates 2 clusters, UserB creates 1 — each sees only their own.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_user_sees_only_own_clusters(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange
		let (_container, _conn, _server, client) = test_app.await;

		let token_a = register_user(&client, "user_a", "a@example.com").await;
		authenticate_client(&client, &token_a).await;
		create_cluster(&client, "cluster-a1").await;
		create_cluster(&client, "cluster-a2").await;

		let token_b = register_user(&client, "user_b", "b@example.com").await;
		authenticate_client(&client, &token_b).await;
		create_cluster(&client, "cluster-b1").await;

		// Act — UserB lists clusters
		let resp_b = client
			.get("/api/clusters/")
			.await
			.expect("List clusters request failed");

		// Assert — UserB sees only 1
		assert_eq!(resp_b.status_code(), 200);
		let body_b: serde_json::Value = resp_b.json().expect("Failed to parse JSON");
		assert_eq!(body_b["total"], 1);
		let items_b = body_b["items"]
			.as_array()
			.expect("items should be an array");
		assert_eq!(items_b.len(), 1);
		assert_eq!(items_b[0]["name"], "cluster-b1");

		// Act — switch to UserA
		authenticate_client(&client, &token_a).await;
		let resp_a = client
			.get("/api/clusters/")
			.await
			.expect("List clusters request failed");

		// Assert — UserA sees 2
		assert_eq!(resp_a.status_code(), 200);
		let body_a: serde_json::Value = resp_a.json().expect("Failed to parse JSON");
		assert_eq!(body_a["total"], 2);
		let items_a = body_a["items"]
			.as_array()
			.expect("items should be an array");
		assert_eq!(items_a.len(), 2);
	}

	/// A token signed with the wrong secret should return 401.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_wrong_secret_token_returns_401(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange
		let (_container, _conn, _server, client) = test_app.await;
		let wrong_auth = JwtAuth::new(b"wrong-secret-that-is-32-bytes-long!");
		let token = wrong_auth
			.generate_token(
				Uuid::new_v4().to_string(),
				"wrong-secret-user".to_string(),
				false,
				false,
			)
			.expect("Failed to generate token");

		authenticate_client(&client, &token).await;

		// Act
		let response = client
			.get("/api/clusters/")
			.await
			.expect("List clusters request failed");

		// Assert
		assert_eq!(response.status_code(), 401);
	}

	/// A malformed bearer token should return 401.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_malformed_bearer_token_returns_401(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange
		let (_container, _conn, _server, client) = test_app.await;
		authenticate_client(&client, "invalid-jwt-gibberish").await;

		// Act
		let response = client
			.get("/api/clusters/")
			.await
			.expect("List clusters request failed");

		// Assert
		assert_eq!(response.status_code(), 401);
	}

	/// Authorization header without "Bearer " prefix should return 401.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_missing_bearer_prefix_returns_401(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange
		let (_container, _conn, _server, client) = test_app.await;
		client
			.set_header("Authorization", "just-a-token-no-bearer".to_string())
			.await
			.expect("Failed to set Authorization header");

		// Act
		let response = client
			.get("/api/clusters/")
			.await
			.expect("List clusters request failed");

		// Assert
		assert_eq!(response.status_code(), 401);
	}
}
