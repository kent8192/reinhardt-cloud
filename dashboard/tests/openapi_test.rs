//! OpenAPI schema generation tests.
//!
//! Verifies that the OpenAPI endpoints (`/api/openapi.json`, `/api/docs`,
//! `/api/redoc`) are available and return valid content when the router
//! is wrapped with `OpenApiRouter`.

// Native-only — see `tests/wasm.rs` for browser tests. Refs #574.
#![cfg(not(target_arch = "wasm32"))]

use reinhardt::UrlReverser;
use reinhardt::test::APIClient;
use reinhardt_cloud_dashboard::config::test_helpers::test_app;
use rstest::rstest;
use std::sync::Arc;

#[rstest]
#[tokio::test(flavor = "multi_thread")]
async fn test_openapi_json_endpoint_returns_valid_spec(test_app: (APIClient, Arc<UrlReverser>)) {
	// Arrange
	let (client, _urls) = test_app;

	// Act
	let response = client
		.get("/api/openapi.json")
		.await
		.expect("GET /api/openapi.json failed");

	// Assert
	assert_eq!(response.status_code(), 200);
	let spec: serde_json::Value = response.json_value().expect("Invalid JSON");
	assert!(
		spec["openapi"].as_str().unwrap_or("").starts_with("3."),
		"Expected OpenAPI 3.x version, got: {:?}",
		spec["openapi"]
	);
}

#[rstest]
#[tokio::test(flavor = "multi_thread")]
async fn test_openapi_spec_contains_registered_schemas(test_app: (APIClient, Arc<UrlReverser>)) {
	// Arrange
	let (client, _urls) = test_app;

	// Act
	let response = client
		.get("/api/openapi.json")
		.await
		.expect("GET /api/openapi.json failed");

	// Assert
	let spec: serde_json::Value = response.json_value().expect("Invalid JSON");
	let schemas = &spec["components"]["schemas"];
	assert!(schemas.is_object(), "Expected schemas object in components");
	assert!(
		spec["paths"].is_object(),
		"Expected paths object in OpenAPI spec"
	);
}

#[rstest]
#[tokio::test(flavor = "multi_thread")]
async fn test_swagger_ui_endpoint(test_app: (APIClient, Arc<UrlReverser>)) {
	// Arrange
	let (client, _urls) = test_app;

	// Act
	let response = client.get("/api/docs").await.expect("GET /api/docs failed");

	// Assert
	assert_eq!(response.status_code(), 200);
	let body = response.text();
	assert!(
		body.contains("swagger") || body.contains("Swagger"),
		"Expected Swagger UI content"
	);
}

#[rstest]
#[tokio::test(flavor = "multi_thread")]
async fn test_redoc_endpoint(test_app: (APIClient, Arc<UrlReverser>)) {
	// Arrange
	let (client, _urls) = test_app;

	// Act
	let response = client
		.get("/api/redoc")
		.await
		.expect("GET /api/redoc failed");

	// Assert
	assert_eq!(response.status_code(), 200);
	let body = response.text();
	assert!(
		body.contains("redoc") || body.contains("Redoc") || body.contains("ReDoc"),
		"Expected Redoc UI content"
	);
}
