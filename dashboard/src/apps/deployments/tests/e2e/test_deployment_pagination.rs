//! Pagination tests for deployments API.

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

	use crate::config::test_helpers::{
		ResolvedUrls, force_login_user_with_org, session_backend, test_app,
	};

	#[fixture]
	async fn db(
		session_backend: Arc<dyn AsyncSessionBackend>,
	) -> (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		ResolvedUrls,
		Arc<dyn AsyncSessionBackend>,
	) {
		// Start the TestContainers database first so that build_test_app() can
		// register the DatabaseConnection in the DI singleton scope. This ensures
		// view handlers that inject Depends<DatabaseConnection> see the same DB
		// as helpers using create_with_conn. Fixes #459.
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations");
		let (client, urls) = crate::config::test_helpers::build_test_app();
		(container, conn, client, urls, session_backend)
	}

	/// Helper: create a cluster and return its id.
	async fn create_cluster(client: &APIClient, org_slug: &str) -> i64 {
		let data = json!({
			"name": "test-cluster",
			"api_url": "https://k8s.example.com:6443"
		});
		let resp = client
			.post(&format!("/api/orgs/{org_slug}/clusters/"), &data, "json")
			.await
			.expect("Create cluster failed");
		assert_eq!(resp.status_code(), 201);
		let body: serde_json::Value = resp.json().expect("Failed to parse cluster response");
		body["id"].as_i64().expect("cluster id")
	}

	/// Helper: create a deployment with a unique app_name.
	async fn create_deployment(client: &APIClient, org_slug: &str, cluster_id: i64, suffix: &str) {
		let data = json!({
			"app_name": format!("app-{suffix}"),
			"cluster_id": cluster_id,
			"image": "nginx:latest"
		});
		let resp = client
			.post(&format!("/api/orgs/{org_slug}/deployments/"), &data, "json")
			.await
			.expect("Create deployment failed");
		assert_eq!(resp.status_code(), 201);
	}

	/// Default pagination returns all items when count is small.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_deployments_default_pagination(
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
		let (_user, org) =
			force_login_user_with_org(&client, &conn, &backend, "testuser", "test@example.com")
				.await;
		let slug = &org.slug;
		let cluster_id = create_cluster(&client, slug).await;
		for i in 1..=3 {
			create_deployment(&client, slug, cluster_id, &i.to_string()).await;
		}

		// Act
		let response = client
			.get(&format!("/api/orgs/{slug}/deployments/"))
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
		let (_user, org) =
			force_login_user_with_org(&client, &conn, &backend, "testuser", "test@example.com")
				.await;
		let slug = &org.slug;
		let cluster_id = create_cluster(&client, slug).await;
		for i in 1..=3 {
			create_deployment(&client, slug, cluster_id, &i.to_string()).await;
		}

		// Act
		let response = client
			.get(&format!("/api/orgs/{slug}/deployments/?page=2&page_size=2"))
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
		let (_user, org) =
			force_login_user_with_org(&client, &conn, &backend, "testuser", "test@example.com")
				.await;
		let slug = &org.slug;
		let cluster_id = create_cluster(&client, slug).await;
		for i in 1..=3 {
			create_deployment(&client, slug, cluster_id, &i.to_string()).await;
		}

		// Act
		let response = client
			.get(&format!("/api/orgs/{slug}/deployments/?page=99"))
			.await
			.expect("List deployments page 99 failed");

		// Assert
		assert_eq!(response.status_code(), 200);
		let body: serde_json::Value = response.json().expect("Failed to parse response");
		let items = body["items"].as_array().expect("items should be array");
		assert_eq!(items.len(), 0);
	}
}
