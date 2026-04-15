//! Cross-user isolation and cluster ownership tests for deployments API.

#[cfg(test)]
mod tests {
	use reinhardt::middleware::session::AsyncSessionBackend;
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::test::APIClient;
	use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage};
	use rstest::*;
	use serde_json::json;
	use serial_test::serial;
	use std::sync::Arc;

	use crate::config::test_helpers::{TestUrls, force_login_user, session_backend, test_app};

	#[fixture]
	async fn db(
		test_app: (APIClient, TestUrls),
		session_backend: Arc<dyn AsyncSessionBackend>,
	) -> (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		TestUrls,
		Arc<dyn AsyncSessionBackend>,
	) {
		let (client, urls) = test_app;
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations");
		(container, conn, client, urls, session_backend)
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
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			TestUrls,
			Arc<dyn AsyncSessionBackend>,
		),
	) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;

		force_login_user(&client, &conn, &backend, "user_a", "a@example.com").await;
		let cluster_id = create_cluster(&client).await;
		create_deployment(&client, cluster_id).await;

		force_login_user(&client, &conn, &backend, "user_b", "b@example.com").await;

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
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			TestUrls,
			Arc<dyn AsyncSessionBackend>,
		),
	) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;
		force_login_user(&client, &conn, &backend, "testuser", "test@example.com").await;

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
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			TestUrls,
			Arc<dyn AsyncSessionBackend>,
		),
	) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;

		// UserA creates a cluster
		force_login_user(&client, &conn, &backend, "owner", "owner@example.com").await;
		let cluster_id = create_cluster(&client).await;

		// UserB tries to deploy to it
		force_login_user(&client, &conn, &backend, "intruder", "intruder@example.com").await;

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
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			TestUrls,
			Arc<dyn AsyncSessionBackend>,
		),
		#[case] cluster_exists: bool,
		#[case] is_owner: bool,
		#[case] expected_status: u16,
	) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;

		let cluster_id = if cluster_exists {
			// Create a cluster owned by user_a
			force_login_user(
				&client,
				&conn,
				&backend,
				"cluster_owner",
				"cowner@example.com",
			)
			.await;
			let cid = create_cluster(&client).await;

			if !is_owner {
				// Deploy as a different user
				force_login_user(&client, &conn, &backend, "deployer", "deployer@example.com")
					.await;
			}
			cid
		} else {
			// Use a nonexistent cluster id
			force_login_user(&client, &conn, &backend, "solo_user", "solo@example.com").await;
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
