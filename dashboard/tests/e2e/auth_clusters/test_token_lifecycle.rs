//! Session lifecycle tests across auth and cluster endpoints.
//!
//! Verifies that session cookies obtained via login are accepted by
//! protected cluster endpoints, and that invalid session cookies are
//! correctly rejected. Users are created directly via ORM to avoid
//! requiring an SMTP server.

use reinhardt::BaseUser;
use reinhardt::db::orm::Model;
use reinhardt::prelude::DatabaseConnection;
use reinhardt::test::APIClient;
use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
use reinhardt::test::fixtures::{ContainerAsync, GenericImage};
use reinhardt_cloud_dashboard::apps::auth::models::User;
use rstest::*;
use serde_json::json;
use serial_test::serial;
use std::sync::Arc;

use reinhardt_cloud_dashboard::config::test_helpers::{TestUrls, test_app};

// ============================================================================
// Fixtures & Helpers
// ============================================================================

#[fixture]
async fn db(
	test_app: (APIClient, TestUrls),
) -> (
	ContainerAsync<GenericImage>,
	Arc<DatabaseConnection>,
	APIClient,
	TestUrls,
) {
	let (client, urls) = test_app;
	let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
	let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
		.await
		.expect("Failed to start PostgreSQL with migrations");
	(container, conn, client, urls)
}

/// Create a user via ORM, then login to obtain a session cookie.
async fn create_user_and_login(client: &APIClient, urls: &TestUrls) -> String {
	// Create active user via ORM (bypasses register endpoint and email)
	let mut user = User::new(
		"testuser".to_string(),
		"test@example.com".to_lowercase(),
		String::new(),
		String::new(),
		None,
		true,
		false,
		false,
	);
	user.set_password("securepassword123")
		.expect("Password hashing failed");
	User::objects()
		.create(&user)
		.await
		.expect("Failed to create user");

	// Login to obtain session cookie
	let login_data = json!({
		"username": "testuser",
		"password": "securepassword123"
	});
	let login_resp = client
		.post(&urls.auth_login, &login_data, "json")
		.await
		.expect("Login request failed");
	assert_eq!(login_resp.status_code(), 200);
	let set_cookie = login_resp
		.header("Set-Cookie")
		.expect("Login response should have Set-Cookie header");
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

/// Login session is accepted by the cluster creation endpoint.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[serial(database)]
async fn test_register_session_works_for_cluster_creation(
	#[future] db: (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		TestUrls,
	),
) {
	// Arrange
	let (_container, _conn, client, urls) = db.await;
	let session = create_user_and_login(&client, &urls).await;
	authenticate_client(&client, &session).await;

	// Act
	let cluster_data = json!({
		"name": "register-session-cluster",
		"api_url": "https://register.k8s.local:6443"
	});
	let resp = client
		.post(&urls.cluster_list, &cluster_data, "json")
		.await
		.expect("Create cluster request failed");

	// Assert
	assert_eq!(resp.status_code(), 201);
}

/// Login session is accepted by the cluster creation endpoint.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[serial(database)]
async fn test_login_session_works_for_cluster_creation(
	#[future] db: (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		TestUrls,
	),
) {
	// Arrange
	let (_container, _conn, client, urls) = db.await;
	let _first_session = create_user_and_login(&client, &urls).await;

	// Login with the same credentials
	let login_data = json!({
		"username": "testuser",
		"password": "securepassword123"
	});
	let login_resp = client
		.post(&urls.auth_login, &login_data, "json")
		.await
		.expect("Login request failed");
	assert_eq!(login_resp.status_code(), 200);
	let set_cookie = login_resp
		.header("Set-Cookie")
		.expect("Login response should have Set-Cookie header");
	let login_session = set_cookie
		.split(';')
		.next()
		.unwrap()
		.strip_prefix("sessionid=")
		.expect("Cookie should start with sessionid=");
	authenticate_client(&client, login_session).await;

	// Act
	let cluster_data = json!({
		"name": "login-session-cluster",
		"api_url": "https://login.k8s.local:6443"
	});
	let resp = client
		.post(&urls.cluster_list, &cluster_data, "json")
		.await
		.expect("Create cluster request failed");

	// Assert
	assert_eq!(resp.status_code(), 201);
}

/// Both sessions give access to the same resources.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[serial(database)]
async fn test_register_and_login_sessions_same_resources(
	#[future] db: (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		TestUrls,
	),
) {
	// Arrange
	let (_container, _conn, client, urls) = db.await;
	let first_session = create_user_and_login(&client, &urls).await;

	// Create a cluster with the first session
	authenticate_client(&client, &first_session).await;
	let cluster_data = json!({
		"name": "shared-cluster",
		"api_url": "https://shared.k8s.local:6443"
	});
	let create_resp = client
		.post(&urls.cluster_list, &cluster_data, "json")
		.await
		.expect("Create cluster failed");
	assert_eq!(create_resp.status_code(), 201);

	// Login to get a new session
	let login_data = json!({
		"username": "testuser",
		"password": "securepassword123"
	});
	let login_resp = client
		.post(&urls.auth_login, &login_data, "json")
		.await
		.expect("Login request failed");
	assert_eq!(login_resp.status_code(), 200);
	let set_cookie = login_resp
		.header("Set-Cookie")
		.expect("Login response should have Set-Cookie header");
	let login_session = set_cookie
		.split(';')
		.next()
		.unwrap()
		.strip_prefix("sessionid=")
		.expect("Cookie should start with sessionid=");

	// Act -- list clusters with the login session
	authenticate_client(&client, login_session).await;
	let list_resp = client
		.get(&urls.cluster_list)
		.await
		.expect("List clusters failed");

	// Assert -- the cluster created with first session is visible
	assert_eq!(list_resp.status_code(), 200);
	let body: serde_json::Value = list_resp.json().expect("Failed to parse list response");
	let items = body["items"].as_array().expect("items should be an array");
	assert_eq!(items.len(), 1);
	assert_eq!(items[0]["name"], "shared-cluster");
}

/// An invalid session cookie must be rejected at resource endpoints.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[serial(database)]
async fn test_invalid_session_rejected_at_resource_endpoint(
	#[future] db: (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		TestUrls,
	),
) {
	// Arrange
	let (_container, _conn, client, urls) = db.await;
	authenticate_client(&client, "invalid-session-id-gibberish").await;

	// Act
	let resp = client
		.get(&urls.cluster_list)
		.await
		.expect("Request failed");

	// Assert
	assert_eq!(resp.status_code(), 401);
}
