//! OpenAPI schema generation tests.
//!
//! Verifies that the OpenAPI endpoints (`/api/openapi.json`, `/api/docs`,
//! `/api/redoc`) are available and return valid content when the router
//! is wrapped with `OpenApiRouter`.

use reinhardt::{Handler, OpenApiRouter, Request, StatusCode};
use rstest::rstest;

use nuages::routes;

/// Create a request and handle it through the OpenAPI-wrapped router.
async fn openapi_get(path: &str) -> reinhardt::Response {
	let router = routes().into_server();
	let wrapped = OpenApiRouter::wrap(router).expect("Failed to wrap with OpenApiRouter");
	let request = Request::builder()
		.uri(path)
		.build()
		.expect("Failed to build request");
	wrapped
		.handle(request)
		.await
		.expect("Handler returned error")
}

#[rstest]
#[tokio::test]
async fn test_openapi_json_endpoint_returns_valid_spec() {
	// Arrange & Act
	let response = openapi_get("/api/openapi.json").await;

	// Assert
	assert_eq!(response.status, StatusCode::OK);
	let body = String::from_utf8(response.body.to_vec()).expect("Invalid UTF-8");
	let spec: serde_json::Value = serde_json::from_str(&body).expect("Invalid JSON");
	assert!(
		spec["openapi"].as_str().unwrap_or("").starts_with("3."),
		"Expected OpenAPI 3.x version, got: {:?}",
		spec["openapi"]
	);
}

#[rstest]
#[tokio::test]
async fn test_openapi_spec_contains_registered_schemas() {
	// Arrange & Act
	let response = openapi_get("/api/openapi.json").await;

	// Assert
	let body = String::from_utf8(response.body.to_vec()).expect("Invalid UTF-8");
	let spec: serde_json::Value = serde_json::from_str(&body).expect("Invalid JSON");
	let schemas = &spec["components"]["schemas"];
	assert!(schemas.is_object(), "Expected schemas object in components");

	let schema_keys: Vec<&str> = schemas
		.as_object()
		.unwrap()
		.keys()
		.map(|k| k.as_str())
		.collect();

	// Auth serializers
	assert!(
		schema_keys.iter().any(|k| k.contains("LoginRequest")),
		"LoginRequest schema missing. Available: {schema_keys:?}"
	);
	assert!(
		schema_keys.iter().any(|k| k.contains("RegisterRequest")),
		"RegisterRequest schema missing. Available: {schema_keys:?}"
	);
	assert!(
		schema_keys.iter().any(|k| k.contains("TokenResponse")),
		"TokenResponse schema missing. Available: {schema_keys:?}"
	);

	// Cluster serializers
	assert!(
		schema_keys
			.iter()
			.any(|k| k.contains("CreateClusterRequest")),
		"CreateClusterRequest schema missing. Available: {schema_keys:?}"
	);
	assert!(
		schema_keys.iter().any(|k| k.contains("ClusterResponse")),
		"ClusterResponse schema missing. Available: {schema_keys:?}"
	);

	// Deployment serializers
	assert!(
		schema_keys
			.iter()
			.any(|k| k.contains("CreateDeploymentRequest")),
		"CreateDeploymentRequest schema missing. Available: {schema_keys:?}"
	);
	assert!(
		schema_keys.iter().any(|k| k.contains("DeploymentResponse")),
		"DeploymentResponse schema missing. Available: {schema_keys:?}"
	);
}

#[rstest]
#[tokio::test]
async fn test_swagger_ui_endpoint() {
	// Arrange & Act
	let response = openapi_get("/api/docs").await;

	// Assert
	assert_eq!(response.status, StatusCode::OK);
	let body = String::from_utf8(response.body.to_vec()).expect("Invalid UTF-8");
	assert!(
		body.contains("swagger") || body.contains("Swagger"),
		"Expected Swagger UI content"
	);
}

#[rstest]
#[tokio::test]
async fn test_redoc_endpoint() {
	// Arrange & Act
	let response = openapi_get("/api/redoc").await;

	// Assert
	assert_eq!(response.status, StatusCode::OK);
	let body = String::from_utf8(response.body.to_vec()).expect("Invalid UTF-8");
	assert!(
		body.contains("redoc") || body.contains("Redoc") || body.contains("ReDoc"),
		"Expected Redoc UI content"
	);
}
