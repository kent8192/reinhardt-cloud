//! Cross-application end-to-end tests for Reinhardt Cloud REST API.
//!
//! Tests that span multiple apps (e.g., creating a deployment
//! requires a cluster) belong here.

use reinhardt::ServerRouter;
use reinhardt::middleware::session::AsyncSessionBackend;
use reinhardt::prelude::DatabaseConnection;
use reinhardt::test::APIClient;
use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
use reinhardt::test::fixtures::{ContainerAsync, GenericImage};
use rstest::*;
use serde_json::json;
use serial_test::serial;
use std::sync::Arc;

use reinhardt_cloud_dashboard::config::test_helpers::{
	force_login_user_with_org, session_backend,
};

// ============================================================================
// Test Fixtures
// ============================================================================

/// PostgreSQL + migrations + test server fixture.
#[fixture]
async fn db(
	session_backend: Arc<dyn AsyncSessionBackend>,
) -> (
	ContainerAsync<GenericImage>,
	Arc<DatabaseConnection>,
	APIClient,
	Arc<ServerRouter>,
	Arc<dyn AsyncSessionBackend>,
) {
	// Start TestContainers first so build_test_app() registers DatabaseConnection
	// in the DI scope. Fixes #459.
	let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
	let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
		.await
		.expect("Failed to start PostgreSQL with migrations");
	let (client, urls) = reinhardt_cloud_dashboard::config::test_helpers::build_test_app();
	(container, conn, client, urls, session_backend)
}

// ============================================================================
// Cross-App Tests (Deployments + Clusters)
// ============================================================================

/// Verify full deployment workflow: create cluster, then deploy to it.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[serial(database)]
async fn test_create_deployment_with_cluster(
	#[future] db: (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		Arc<ServerRouter>,
		Arc<dyn AsyncSessionBackend>,
	),
) {
	// Arrange
	let (_container, conn, client, _urls, backend) = db.await;
	let (_user, org) =
		force_login_user_with_org(&client, &conn, &backend, "testuser", "test@example.com").await;
	let slug = &org.slug;

	// Act -- create a cluster first (deployment requires cluster_id)
	let cluster_data = json!({
		"name": "staging-cluster",
		"api_url": "https://staging.k8s.local:6443"
	});
	let cluster_response = client
		.post(
			&format!("/api/orgs/{slug}/clusters/"),
			&cluster_data,
			"json",
		)
		.await
		.expect("Create cluster request failed");
	assert_eq!(cluster_response.status_code(), 201);
	let cluster: serde_json::Value = cluster_response
		.json()
		.expect("Failed to parse cluster response");
	let cluster_id = cluster["id"].as_i64().expect("Cluster id should be i64");

	// Act -- create deployment referencing the cluster
	let deployment_data = json!({
		"app_name": "my-web-app",
		"cluster_id": cluster_id,
		"image": "registry.example.com/my-web-app:v1.0.0"
	});
	let create_response = client
		.post(
			&format!("/api/orgs/{slug}/deployments/"),
			&deployment_data,
			"json",
		)
		.await
		.expect("Create deployment request failed");

	// Assert -- creation response
	assert_eq!(create_response.status_code(), 201);
	let created: serde_json::Value = create_response
		.json()
		.expect("Failed to parse create response");
	assert_eq!(created["app_name"], "my-web-app");
	assert_eq!(created["cluster_id"], cluster_id);
	assert_eq!(created["status"], "pending");
	assert_eq!(created["image"], "registry.example.com/my-web-app:v1.0.0");
	assert!(created["id"].is_number());

	// Act -- list deployments to verify persistence
	let list_response = client
		.get(&format!("/api/orgs/{slug}/deployments/"))
		.await
		.expect("List deployments request failed");

	// Assert -- deployment appears in list
	assert_eq!(list_response.status_code(), 200);
	let body: serde_json::Value = list_response.json().expect("Failed to parse list response");
	let items = body["items"].as_array().expect("items should be an array");
	assert_eq!(items.len(), 1);
	assert_eq!(items[0]["app_name"], "my-web-app");
	assert_eq!(items[0]["cluster_id"], cluster_id);
	assert_eq!(items[0]["status"], "pending");
	assert_eq!(body["total"], 1);
	assert!(body["page"].is_number());
	assert!(body["page_size"].is_number());
	let list_id = items[0]["id"]
		.as_i64()
		.expect("Deployment id should be present in list response");
	assert!(
		list_id > 0,
		"Deployment id should be a positive database-generated value, got {list_id}"
	);
}
