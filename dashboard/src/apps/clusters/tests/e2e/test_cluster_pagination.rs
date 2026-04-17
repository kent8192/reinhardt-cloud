//! Pagination tests for clusters API.

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

	use crate::config::test_helpers::{ResolvedUrls, force_login_user, session_backend, test_app};

	#[fixture]
	async fn db(
		test_app: (APIClient, ResolvedUrls),
		session_backend: Arc<dyn AsyncSessionBackend>,
	) -> (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		ResolvedUrls,
		Arc<dyn AsyncSessionBackend>,
	) {
		let (client, urls) = test_app;
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations");
		(container, conn, client, urls, session_backend)
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
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
			Arc<dyn AsyncSessionBackend>,
		),
	) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;
		force_login_user(&client, &conn, &backend, "testuser", "test@example.com").await;
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
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
			Arc<dyn AsyncSessionBackend>,
		),
	) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;
		force_login_user(&client, &conn, &backend, "testuser", "test@example.com").await;
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
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
			Arc<dyn AsyncSessionBackend>,
		),
	) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;
		force_login_user(&client, &conn, &backend, "testuser", "test@example.com").await;
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
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
			Arc<dyn AsyncSessionBackend>,
		),
	) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;
		force_login_user(&client, &conn, &backend, "testuser", "test@example.com").await;

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
