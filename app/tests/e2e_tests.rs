//! End-to-end tests for nuages REST API endpoints.
//!
//! Uses TestContainers PostgreSQL for database integration and
//! reinhardt test server for HTTP endpoint testing.

use reinhardt::db::migrations::MigrationProvider;
use reinhardt::db::migrations::executor::DatabaseMigrationExecutor;
use reinhardt::db::orm::reinitialize_database;
use reinhardt::prelude::DatabaseConnection;
use reinhardt::test::fixtures::{ContainerAsync, GenericImage, api_client_from_url};
use reinhardt::test::fixtures::{postgres_container, test_server_guard};
use reinhardt::test::fixtures::TestServerGuard;
use reinhardt::test::APIClient;
use rstest::*;
use serde_json::json;
use serial_test::serial;
use std::sync::Arc;

use nuages::{NuagesMigrations, routes};

// ============================================================================
// Test Fixtures
// ============================================================================

/// PostgreSQL + migrations + test server fixture.
///
/// Sets up a complete test environment:
/// 1. Starts PostgreSQL container via TestContainers
/// 2. Applies all nuages migrations
/// 3. Initializes global database state for ORM operations
/// 4. Starts HTTP test server with nuages routes
#[fixture]
async fn test_app() -> (
	ContainerAsync<GenericImage>,
	Arc<DatabaseConnection>,
	TestServerGuard,
	APIClient,
) {
	// Start PostgreSQL container
	let (container, _pool, _port, database_url) = postgres_container().await;

	// Connect and apply migrations
	let conn = DatabaseConnection::connect(&database_url)
		.await
		.expect("Failed to connect to PostgreSQL");

	let migrations = NuagesMigrations::migrations();
	if !migrations.is_empty() {
		let mut executor = DatabaseMigrationExecutor::new(conn.inner().clone());
		executor
			.apply_migrations(&migrations)
			.await
			.expect("Failed to apply migrations");
	}

	// Initialize global database state for Manager<T>
	reinitialize_database(&database_url)
		.await
		.expect("Failed to initialize global database state");

	// Start test server with nuages routes
	let router = routes().into_server();
	let server = test_server_guard(router).await;

	// Create API client pointing to test server
	let client = api_client_from_url(&server.url);

	(container, Arc::new(conn), server, client)
}

// ============================================================================
// Auth Endpoint Tests
// ============================================================================

/// Verify POST /api/auth/login/ returns a JWT bearer token.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[serial(database)]
async fn test_login_returns_jwt_token(
	#[future] test_app: (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		TestServerGuard,
		APIClient,
	),
) {
	// Arrange
	let (_container, _conn, _server, client) = test_app.await;
	let login_data = json!({
		"username": "testuser",
		"password": "testpassword"
	});

	// Act
	let response = client
		.post("/api/auth/login/", &login_data, "json")
		.await
		.expect("Login request failed");

	// Assert
	assert_eq!(response.status_code(), 200);
	let body: serde_json::Value = response.json().expect("Failed to parse JSON response");
	assert_eq!(body["token_type"], "Bearer");
	assert!(body["token"].is_string());
	assert!(!body["token"].as_str().unwrap().is_empty());
}

/// Verify POST /api/auth/register/ returns a JWT bearer token with 201 status.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[serial(database)]
async fn test_register_returns_jwt_token(
	#[future] test_app: (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		TestServerGuard,
		APIClient,
	),
) {
	// Arrange
	let (_container, _conn, _server, client) = test_app.await;
	let register_data = json!({
		"username": "newuser",
		"email": "newuser@example.com",
		"password": "securepassword"
	});

	// Act
	let response = client
		.post("/api/auth/register/", &register_data, "json")
		.await
		.expect("Register request failed");

	// Assert
	assert_eq!(response.status_code(), 201);
	let body: serde_json::Value = response.json().expect("Failed to parse JSON response");
	assert_eq!(body["token_type"], "Bearer");
	assert!(body["token"].is_string());
	assert!(!body["token"].as_str().unwrap().is_empty());
}

// ============================================================================
// Clusters Endpoint Tests
// ============================================================================

/// Verify GET /api/clusters/ returns empty list initially.
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
	let created: serde_json::Value =
		create_response.json().expect("Failed to parse create response");
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

// ============================================================================
// Deployments Endpoint Tests
// ============================================================================

/// Verify GET /api/deployments/ returns empty list initially.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[serial(database)]
async fn test_list_deployments_empty(
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
		.get("/api/deployments/")
		.await
		.expect("List deployments request failed");

	// Assert
	assert_eq!(response.status_code(), 200);
	let body: Vec<serde_json::Value> = response.json().expect("Failed to parse JSON response");
	assert_eq!(body.len(), 0);
}

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
	let cluster: serde_json::Value =
		cluster_response.json().expect("Failed to parse cluster response");
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
	let created: serde_json::Value =
		create_response.json().expect("Failed to parse create response");
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
}
