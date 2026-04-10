//! OpenAPI schema generation tests.
//!
//! Verifies that the OpenAPI endpoints (`/api/openapi.json`, `/api/docs`,
//! `/api/redoc`) are available and return valid content when the router
//! is wrapped with `OpenApiRouter`.

use reinhardt::test::APIClient;
use reinhardt::{Handler, Request, StatusCode};
use reinhardt_cloud_dashboard::config::test_helpers::{TestUrls, test_app};
use rstest::rstest;

/// Create a request and handle it through the test app router.
async fn openapi_get(client: &APIClient, path: &str) -> reinhardt::Response {
	let request = Request::builder()
		.uri(path)
		.header("Origin", "http://testserver")
		.build()
		.expect("Failed to build request");
	client
		.handle(request)
		.await
		.expect("Handler returned error")
}

#[rstest]
#[tokio::test(flavor = "multi_thread")]
async fn test_openapi_json_endpoint_returns_valid_spec(test_app: (APIClient, TestUrls)) {
	// Arrange
	let (client, _urls) = test_app;

	// Act
	let response = openapi_get(&client, "/api/openapi.json").await;

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
#[tokio::test(flavor = "multi_thread")]
async fn test_openapi_spec_contains_registered_schemas(test_app: (APIClient, TestUrls)) {
	// Arrange
	let (client, _urls) = test_app;

	// Act
	let response = openapi_get(&client, "/api/openapi.json").await;

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

	// Auth serializers (cookie-session auth — no TokenResponse)
	assert!(
		schema_keys.iter().any(|k| k.contains("LoginRequest")),
		"LoginRequest schema missing. Available: {schema_keys:?}"
	);
	assert!(
		schema_keys.iter().any(|k| k.contains("RegisterRequest")),
		"RegisterRequest schema missing. Available: {schema_keys:?}"
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
#[tokio::test(flavor = "multi_thread")]
async fn test_swagger_ui_endpoint(test_app: (APIClient, TestUrls)) {
	// Arrange
	let (client, _urls) = test_app;

	// Act
	let response = openapi_get(&client, "/api/docs").await;

	// Assert
	assert_eq!(response.status, StatusCode::OK);
	let body = String::from_utf8(response.body.to_vec()).expect("Invalid UTF-8");
	assert!(
		body.contains("swagger") || body.contains("Swagger"),
		"Expected Swagger UI content"
	);
}

#[rstest]
#[tokio::test(flavor = "multi_thread")]
async fn test_redoc_endpoint(test_app: (APIClient, TestUrls)) {
	// Arrange
	let (client, _urls) = test_app;

	// Act
	let response = openapi_get(&client, "/api/redoc").await;

	// Assert
	assert_eq!(response.status, StatusCode::OK);
	let body = String::from_utf8(response.body.to_vec()).expect("Invalid UTF-8");
	assert!(
		body.contains("redoc") || body.contains("Redoc") || body.contains("ReDoc"),
		"Expected Redoc UI content"
	);
}
