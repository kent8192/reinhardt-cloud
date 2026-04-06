//! JWT token lifecycle tests across auth and cluster endpoints.
//!
//! Verifies that tokens obtained via register and login are both
//! accepted by protected cluster endpoints, and that tampered
//! tokens are correctly rejected.

use reinhardt::prelude::DatabaseConnection;
use reinhardt::test::APIClient;
use reinhardt::test::fixtures::TestServerGuard;
use reinhardt::test::fixtures::{ContainerAsync, GenericImage, api_client_from_url};
use reinhardt::test::fixtures::{postgres_with_migrations_from_dir, test_server_guard};
use rstest::*;
use serde_json::json;
use serial_test::serial;
use std::sync::Arc;

use reinhardt_cloud_dashboard::routes;

// ============================================================================
// Fixtures & Helpers
// ============================================================================

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

async fn authenticate_client(client: &APIClient, token: &str) {
	client
		.set_header("Authorization", format!("Bearer {token}"))
		.await
		.expect("Failed to set Authorization header");
}

// ============================================================================
// Tests
// ============================================================================

/// Register token is accepted by the cluster creation endpoint.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[serial(database)]
async fn test_register_token_works_for_cluster_creation(
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
	let cluster_data = json!({
		"name": "register-token-cluster",
		"api_url": "https://register.k8s.local:6443"
	});
	let resp = client
		.post("/api/clusters/", &cluster_data, "json")
		.await
		.expect("Create cluster request failed");

	// Assert
	assert_eq!(resp.status_code(), 201);
}

/// Login token is accepted by the cluster creation endpoint.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[serial(database)]
async fn test_login_token_works_for_cluster_creation(
	#[future] test_app: (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		TestServerGuard,
		APIClient,
	),
) {
	// Arrange
	let (_container, _conn, _server, client) = test_app.await;
	let _register_token = register_and_get_token(&client).await;

	// Login with the same credentials
	let login_data = json!({
		"username": "testuser",
		"password": "securepassword123"
	});
	let login_resp = client
		.post("/api/auth/login/", &login_data, "json")
		.await
		.expect("Login request failed");
	assert_eq!(login_resp.status_code(), 200);
	let login_body: serde_json::Value = login_resp.json().expect("Failed to parse login response");
	let login_token = login_body["token"].as_str().unwrap();
	authenticate_client(&client, login_token).await;

	// Act
	let cluster_data = json!({
		"name": "login-token-cluster",
		"api_url": "https://login.k8s.local:6443"
	});
	let resp = client
		.post("/api/clusters/", &cluster_data, "json")
		.await
		.expect("Create cluster request failed");

	// Assert
	assert_eq!(resp.status_code(), 201);
}

/// Register and login tokens both give access to the same resources.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[serial(database)]
async fn test_register_and_login_tokens_same_resources(
	#[future] test_app: (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		TestServerGuard,
		APIClient,
	),
) {
	// Arrange
	let (_container, _conn, _server, client) = test_app.await;
	let register_token = register_and_get_token(&client).await;

	// Create a cluster with the register token
	authenticate_client(&client, &register_token).await;
	let cluster_data = json!({
		"name": "shared-cluster",
		"api_url": "https://shared.k8s.local:6443"
	});
	let create_resp = client
		.post("/api/clusters/", &cluster_data, "json")
		.await
		.expect("Create cluster failed");
	assert_eq!(create_resp.status_code(), 201);

	// Login to get a new token
	let login_data = json!({
		"username": "testuser",
		"password": "securepassword123"
	});
	let login_resp = client
		.post("/api/auth/login/", &login_data, "json")
		.await
		.expect("Login request failed");
	assert_eq!(login_resp.status_code(), 200);
	let login_body: serde_json::Value = login_resp.json().expect("Failed to parse login response");
	let login_token = login_body["token"].as_str().unwrap();

	// Act — list clusters with the login token
	authenticate_client(&client, login_token).await;
	let list_resp = client
		.get("/api/clusters/")
		.await
		.expect("List clusters failed");

	// Assert — the cluster created with register token is visible
	assert_eq!(list_resp.status_code(), 200);
	let body: serde_json::Value = list_resp.json().expect("Failed to parse list response");
	let items = body["items"].as_array().expect("items should be an array");
	assert_eq!(items.len(), 1);
	assert_eq!(items[0]["name"], "shared-cluster");
}

/// A tampered token must be rejected at resource endpoints.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[serial(database)]
async fn test_tampered_token_rejected_at_resource_endpoint(
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

	// Tamper with the token by flipping a character
	let mut tampered = token.clone();
	let bytes = unsafe { tampered.as_bytes_mut() };
	let last = bytes.len() - 1;
	bytes[last] = if bytes[last] == b'a' { b'b' } else { b'a' };
	authenticate_client(&client, &tampered).await;

	// Act
	let resp = client.get("/api/clusters/").await.expect("Request failed");

	// Assert
	assert_eq!(resp.status_code(), 401);
}
