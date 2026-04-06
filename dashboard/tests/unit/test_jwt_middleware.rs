//! Unit and E2E tests for JwtAuthMiddleware.
//!
//! Verifies that `should_continue()` correctly classifies public vs
//! protected paths, and that the middleware enforces authentication
//! on protected endpoints at the HTTP level.

use reinhardt::{Middleware, Request};
use reinhardt_cloud_dashboard::config::middleware::JwtAuthMiddleware;
use rstest::*;

// ============================================================================
// Unit: should_continue decision table
// ============================================================================

#[rstest]
#[case("/api/auth/login/", false)]
#[case("/api/auth/register/", false)]
#[case("/api/openapi.json", false)]
#[case("/api/docs", false)]
#[case("/api/docs/", false)]
#[case("/api/redoc", false)]
#[case("/api/redoc/", false)]
#[case("/api/clusters/", true)]
#[case("/api/deployments/", true)]
fn test_jwt_middleware_skip_paths(#[case] path: &str, #[case] should_enforce: bool) {
	// Arrange
	let middleware = JwtAuthMiddleware;
	let request = Request::builder().uri(path).build().expect("build request");

	// Act
	let result = middleware.should_continue(&request);

	// Assert
	assert_eq!(result, should_enforce, "path={path}");
}

// ============================================================================
// E2E: middleware enforcement via test server
// ============================================================================

use reinhardt::prelude::DatabaseConnection;
use reinhardt::test::APIClient;
use reinhardt::test::fixtures::TestServerGuard;
use reinhardt::test::fixtures::{ContainerAsync, GenericImage, api_client_from_url};
use reinhardt::test::fixtures::{postgres_with_migrations_from_dir, test_server_guard};
use serde_json::json;
use serial_test::serial;
use std::sync::Arc;

use reinhardt_cloud_dashboard::routes;

/// PostgreSQL + migrations + test server fixture.
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

/// Protected endpoint must reject requests without an Authorization header.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[serial(database)]
async fn test_jwt_middleware_rejects_no_auth_header(
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
	let resp = client.get("/api/clusters/").await.expect("Request failed");

	// Assert
	assert_eq!(resp.status_code(), 401);
}

/// Protected endpoint must reject requests with an invalid JWT token.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[serial(database)]
async fn test_jwt_middleware_rejects_invalid_token(
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
		.set_header("Authorization", "Bearer invalid-jwt-token".to_string())
		.await
		.expect("Failed to set header");

	// Act
	let resp = client.get("/api/clusters/").await.expect("Request failed");

	// Assert
	assert_eq!(resp.status_code(), 401);
}

/// Protected endpoint must accept requests with a valid JWT token.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[serial(database)]
async fn test_jwt_middleware_accepts_valid_token(
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
	let resp = client.get("/api/clusters/").await.expect("Request failed");

	// Assert
	assert_eq!(resp.status_code(), 200);
}
