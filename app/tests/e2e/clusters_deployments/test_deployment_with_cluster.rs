//! Cross-application end-to-end tests for nuages REST API.
//!
//! Tests that span multiple apps (e.g., creating a deployment
//! requires a cluster) belong here.

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

use nuages::routes;

// ============================================================================
// Test Fixtures
// ============================================================================

/// PostgreSQL + migrations + test server fixture.
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
	let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
	let source = FilesystemSource::new(migrations_dir);
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

// ============================================================================
// Cross-App Tests (Deployments + Clusters)
// ============================================================================

/// Verify full deployment workflow: create cluster, then deploy to it.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[serial(database)]
async fn test_create_deployment_with_cluster(
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

	// Act — create a cluster first (deployment requires cluster_id)
	let cluster_data = json!({
		"name": "staging-cluster",
		"api_url": "https://staging.k8s.local:6443"
	});
	let cluster_response = client
		.post("/api/clusters/", &cluster_data, "json")
		.await
		.expect("Create cluster request failed");
	assert_eq!(cluster_response.status_code(), 201);
	let cluster: serde_json::Value = cluster_response
		.json()
		.expect("Failed to parse cluster response");
	let cluster_id = cluster["id"].as_i64().expect("Cluster id should be i64");

	// Act — create deployment referencing the cluster
	let deployment_data = json!({
		"app_name": "my-web-app",
		"cluster_id": cluster_id,
		"image": "registry.example.com/my-web-app:v1.0.0"
	});
	let create_response = client
		.post("/api/deployments/", &deployment_data, "json")
		.await
		.expect("Create deployment request failed");

	// Assert — creation response
	assert_eq!(create_response.status_code(), 201);
	let created: serde_json::Value = create_response
		.json()
		.expect("Failed to parse create response");
	assert_eq!(created["app_name"], "my-web-app");
	assert_eq!(created["cluster_id"], cluster_id);
	assert_eq!(created["status"], "pending");
	assert_eq!(created["image"], "registry.example.com/my-web-app:v1.0.0");
	assert!(created["id"].is_number());

	// Act — list deployments to verify persistence
	let list_response = client
		.get("/api/deployments/")
		.await
		.expect("List deployments request failed");

	// Assert — deployment appears in list
	assert_eq!(list_response.status_code(), 200);
	let deployments: Vec<serde_json::Value> =
		list_response.json().expect("Failed to parse list response");
	assert_eq!(deployments.len(), 1);
	assert_eq!(deployments[0]["app_name"], "my-web-app");
	assert_eq!(deployments[0]["cluster_id"], cluster_id);
	assert_eq!(deployments[0]["status"], "pending");
	assert!(
		deployments[0]["id"].as_i64().is_some(),
		"Deployment id should be present in list response"
	);
}
