//! Full user journey tests spanning auth, clusters, and deployments.
//!
//! Verifies the complete workflow: register -> create cluster -> create
//! deployment -> list deployments, and ensures two independent users
//! have complete resource isolation.

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
	let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
	let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
		.await
		.expect("Failed to start PostgreSQL with migrations");
	let router = routes().into_server();
	let server = test_server_guard(router).await;
	let client = api_client_from_url(&server.url);
	client.set_header("Origin", &server.url);
	(container, conn, server, client)
}

async fn register_user(client: &APIClient, username: &str, email: &str) -> String {
	let data = json!({
		"username": username,
		"email": email,
		"password": "securepassword123"
	});
	let resp = client
		.post("/api/auth/register/", &data, "json")
		.await
		.expect("Register request failed");
	assert_eq!(resp.status_code(), 201);
	let set_cookie = resp
		.header("Set-Cookie")
		.expect("Response should have Set-Cookie header");
	let session_id = set_cookie
		.split(';')
		.next()
		.unwrap()
		.strip_prefix("sessionid=")
		.expect("Cookie should start with sessionid=");
	session_id.to_string()
}

async fn authenticate_client(client: &APIClient, session_id: &str) {
	client
		.set_header("Cookie", format!("sessionid={session_id}"))
		.await
		.expect("Failed to set Cookie header");
}

// ============================================================================
// Tests
// ============================================================================

/// Full user journey: register -> create cluster -> create deployment -> list.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[serial(database)]
async fn test_full_user_journey(
	#[future] test_app: (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		TestServerGuard,
		APIClient,
	),
) {
	// Arrange
	let (_container, _conn, _server, client) = test_app.await;
	let session = register_user(&client, "journeyuser", "journey@example.com").await;
	authenticate_client(&client, &session).await;

	// Act -- create cluster
	let cluster_data = json!({
		"name": "journey-cluster",
		"api_url": "https://journey.k8s.local:6443"
	});
	let cluster_resp = client
		.post("/api/clusters/", &cluster_data, "json")
		.await
		.expect("Create cluster failed");
	assert_eq!(cluster_resp.status_code(), 201);
	let cluster: serde_json::Value = cluster_resp.json().expect("Failed to parse cluster");
	let cluster_id = cluster["id"].as_i64().expect("Cluster id should be i64");

	// Act -- create deployment
	let deployment_data = json!({
		"app_name": "journey-app",
		"cluster_id": cluster_id,
		"image": "registry.example.com/journey-app:v1.0.0"
	});
	let deploy_resp = client
		.post("/api/deployments/", &deployment_data, "json")
		.await
		.expect("Create deployment failed");
	assert_eq!(deploy_resp.status_code(), 201);
	let deployment: serde_json::Value = deploy_resp.json().expect("Failed to parse deployment");

	// Assert -- deployment fields match
	assert_eq!(deployment["app_name"], "journey-app");
	assert_eq!(deployment["cluster_id"], cluster_id);
	assert_eq!(
		deployment["image"],
		"registry.example.com/journey-app:v1.0.0"
	);
	assert_eq!(deployment["status"], "pending");
	assert!(deployment["id"].is_number());

	// Act -- list deployments
	let list_resp = client
		.get("/api/deployments/")
		.await
		.expect("List deployments failed");

	// Assert -- deployment appears in list with correct fields
	assert_eq!(list_resp.status_code(), 200);
	let body: serde_json::Value = list_resp.json().expect("Failed to parse list response");
	let items = body["items"].as_array().expect("items should be an array");
	assert_eq!(items.len(), 1);
	assert_eq!(items[0]["app_name"], "journey-app");
	assert_eq!(items[0]["cluster_id"], cluster_id);
	assert_eq!(items[0]["image"], "registry.example.com/journey-app:v1.0.0");
	assert_eq!(body["total"], 1);
}

/// Two users register independently, create their own resources, and
/// each sees only their own clusters and deployments.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[serial(database)]
async fn test_two_users_independent_workflows(
	#[future] test_app: (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		TestServerGuard,
		APIClient,
	),
) {
	// Arrange
	let (_container, _conn, server, client) = test_app.await;

	// --- User A ---
	let session_a = register_user(&client, "user_a", "a@example.com").await;
	authenticate_client(&client, &session_a).await;

	let cluster_a = json!({
		"name": "cluster-a",
		"api_url": "https://a.k8s.local:6443"
	});
	let resp = client
		.post("/api/clusters/", &cluster_a, "json")
		.await
		.expect("Create cluster A failed");
	assert_eq!(resp.status_code(), 201);
	let cluster_a_body: serde_json::Value = resp.json().expect("parse cluster A");
	let cluster_a_id = cluster_a_body["id"].as_i64().unwrap();

	let deploy_a = json!({
		"app_name": "app-a",
		"cluster_id": cluster_a_id,
		"image": "registry.example.com/app-a:v1"
	});
	let resp = client
		.post("/api/deployments/", &deploy_a, "json")
		.await
		.expect("Create deployment A failed");
	assert_eq!(resp.status_code(), 201);

	// --- User B (new client to reset headers) ---
	let client_b = api_client_from_url(&server.url);
	let session_b = register_user(&client_b, "user_b", "b@example.com").await;
	authenticate_client(&client_b, &session_b).await;

	let cluster_b = json!({
		"name": "cluster-b",
		"api_url": "https://b.k8s.local:6443"
	});
	let resp = client_b
		.post("/api/clusters/", &cluster_b, "json")
		.await
		.expect("Create cluster B failed");
	assert_eq!(resp.status_code(), 201);
	let cluster_b_body: serde_json::Value = resp.json().expect("parse cluster B");
	let cluster_b_id = cluster_b_body["id"].as_i64().unwrap();

	let deploy_b = json!({
		"app_name": "app-b",
		"cluster_id": cluster_b_id,
		"image": "registry.example.com/app-b:v1"
	});
	let resp = client_b
		.post("/api/deployments/", &deploy_b, "json")
		.await
		.expect("Create deployment B failed");
	assert_eq!(resp.status_code(), 201);

	// Assert -- User A sees only their resources
	authenticate_client(&client, &session_a).await;
	let list_a = client
		.get("/api/clusters/")
		.await
		.expect("List clusters A failed");
	assert_eq!(list_a.status_code(), 200);
	let body_a: serde_json::Value = list_a.json().expect("parse clusters A");
	let items_a = body_a["items"].as_array().expect("items array");
	assert_eq!(items_a.len(), 1);
	assert_eq!(items_a[0]["name"], "cluster-a");

	let dep_list_a = client
		.get("/api/deployments/")
		.await
		.expect("List deployments A failed");
	assert_eq!(dep_list_a.status_code(), 200);
	let dep_body_a: serde_json::Value = dep_list_a.json().expect("parse deployments A");
	let dep_items_a = dep_body_a["items"].as_array().expect("items array");
	assert_eq!(dep_items_a.len(), 1);
	assert_eq!(dep_items_a[0]["app_name"], "app-a");

	// Assert -- User B sees only their resources
	let list_b = client_b
		.get("/api/clusters/")
		.await
		.expect("List clusters B failed");
	assert_eq!(list_b.status_code(), 200);
	let body_b: serde_json::Value = list_b.json().expect("parse clusters B");
	let items_b = body_b["items"].as_array().expect("items array");
	assert_eq!(items_b.len(), 1);
	assert_eq!(items_b[0]["name"], "cluster-b");

	let dep_list_b = client_b
		.get("/api/deployments/")
		.await
		.expect("List deployments B failed");
	assert_eq!(dep_list_b.status_code(), 200);
	let dep_body_b: serde_json::Value = dep_list_b.json().expect("parse deployments B");
	let dep_items_b = dep_body_b["items"].as_array().expect("items array");
	assert_eq!(dep_items_b.len(), 1);
	assert_eq!(dep_items_b[0]["app_name"], "app-b");
}
