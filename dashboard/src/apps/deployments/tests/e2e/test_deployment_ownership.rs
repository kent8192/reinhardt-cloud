//! Cross-user isolation and cluster ownership tests for deployments API.

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
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations");
		let router = routes().into_server();
		let server = test_server_guard(router).await;
		let client = api_client_from_url(&server.url);
		// Set Origin header so OriginGuardMiddleware accepts POST requests
		client
			.set_header("Origin", &server.url)
			.await
			.expect("Failed to set Origin header");
		(container, conn, server, client)
	}

	/// Helper: register a user and return the session cookie value.
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
		let set_cookie = resp
			.header("Set-Cookie")
			.expect("Response should have Set-Cookie header");
		let session_id = set_cookie
			.split(';')
			.next()
			.unwrap()
			.strip_prefix("sessionid=")
			.expect("Cookie should start with sessionid=");
		session_id.to_string()
	}

	/// Helper: set session cookie on client.
	async fn authenticate_client(client: &APIClient, session_id: &str) {
		client
			.set_header("Cookie", format!("sessionid={session_id}"))
			.await
			.expect("Failed to set Cookie header");
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

	/// Helper: create a deployment and return the response body.
	async fn create_deployment(client: &APIClient, cluster_id: i64) -> serde_json::Value {
		let data = json!({
			"app_name": "test-app",
			"cluster_id": cluster_id,
			"image": "nginx:latest"
		});
		let resp = client
			.post("/api/deployments/", &data, "json")
			.await
			.expect("Create deployment failed");
		assert_eq!(resp.status_code(), 201);
		resp.json().expect("Failed to parse deployment response")
	}

	/// UserA creates a deployment; UserB should not see it.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_user_cannot_see_other_users_deployments(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange
		let (_container, _conn, _server, client) = test_app.await;

		let session_a = register_user(&client, "user_a", "a@example.com").await;
		authenticate_client(&client, &session_a).await;
		let cluster_id = create_cluster(&client).await;
		create_deployment(&client, cluster_id).await;

		let session_b = register_user(&client, "user_b", "b@example.com").await;
		authenticate_client(&client, &session_b).await;

		// Act
		let response = client
			.get("/api/deployments/")
			.await
			.expect("List deployments failed");

		// Assert
		assert_eq!(response.status_code(), 200);
		let body: serde_json::Value = response.json().expect("Failed to parse response");
		assert_eq!(body["items"], json!([]));
		assert_eq!(body["total"], 0);
	}

	/// Creating a deployment with a nonexistent cluster returns 404.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_create_deployment_nonexistent_cluster_404(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange
		let (_container, _conn, _server, client) = test_app.await;
		let session = register_user(&client, "testuser", "test@example.com").await;
		authenticate_client(&client, &session).await;

		let data = json!({
			"app_name": "my-app",
			"cluster_id": 99999,
			"image": "nginx:latest"
		});

		// Act
		let response = client
			.post("/api/deployments/", &data, "json")
			.await
			.expect("Create deployment request failed");

		// Assert
		assert_eq!(response.status_code(), 404);
		let body_text = response.text();
		assert!(
			body_text.contains("Cluster with id 99999 not found")
				|| body_text.contains("not found")
				|| body_text.contains("Not Found"),
			"Expected cluster-not-found message, got: {body_text}"
		);
	}

	/// UserB cannot deploy to UserA's cluster -- returns 404.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_create_deployment_other_users_cluster_404(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
	) {
		// Arrange
		let (_container, _conn, _server, client) = test_app.await;

		// UserA creates a cluster
		let session_a = register_user(&client, "owner", "owner@example.com").await;
		authenticate_client(&client, &session_a).await;
		let cluster_id = create_cluster(&client).await;

		// UserB tries to deploy to it
		let session_b = register_user(&client, "intruder", "intruder@example.com").await;
		authenticate_client(&client, &session_b).await;

		let data = json!({
			"app_name": "evil-app",
			"cluster_id": cluster_id,
			"image": "malicious:latest"
		});

		// Act
		let response = client
			.post("/api/deployments/", &data, "json")
			.await
			.expect("Create deployment request failed");

		// Assert
		assert_eq!(response.status_code(), 404);
		let body_text = response.text();
		assert!(
			body_text.contains(&format!("Cluster with id {cluster_id} not found"))
				|| body_text.contains("not found")
				|| body_text.contains("Not Found"),
			"Expected cluster-not-found for other user's cluster, got: {body_text}"
		);
	}

	/// Decision table: deployment ownership scenarios.
	#[rstest]
	#[case::own_cluster(true, true, 201)]
	#[case::other_users_cluster(true, false, 404)]
	#[case::nonexistent_cluster(false, false, 404)]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_deployment_ownership_decision_table(
		#[future] test_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			TestServerGuard,
			APIClient,
		),
		#[case] cluster_exists: bool,
		#[case] is_owner: bool,
		#[case] expected_status: u16,
	) {
		// Arrange
		let (_container, _conn, _server, client) = test_app.await;

		let cluster_id = if cluster_exists {
			// Create a cluster owned by user_a
			let session_a = register_user(&client, "cluster_owner", "cowner@example.com").await;
			authenticate_client(&client, &session_a).await;
			let cid = create_cluster(&client).await;

			if !is_owner {
				// Deploy as a different user
				let session_b = register_user(&client, "deployer", "deployer@example.com").await;
				authenticate_client(&client, &session_b).await;
			}
			cid
		} else {
			// Use a nonexistent cluster id
			let session = register_user(&client, "solo_user", "solo@example.com").await;
			authenticate_client(&client, &session).await;
			99999i64
		};

		let data = json!({
			"app_name": "decision-app",
			"cluster_id": cluster_id,
			"image": "nginx:latest"
		});

		// Act
		let response = client
			.post("/api/deployments/", &data, "json")
			.await
			.expect("Create deployment request failed");

		// Assert
		assert_eq!(
			response.status_code(),
			expected_status,
			"cluster_exists={cluster_exists}, is_owner={is_owner}"
		);
	}
}
