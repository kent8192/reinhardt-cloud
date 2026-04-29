//! Cross-user ownership isolation tests for clusters and deployments.
//!
//! Verifies that users cannot see each other's clusters or deployments,
//! and that multiple deployments on the same cluster are handled correctly.

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
	ResolvedUrls, force_login_user_with_org, session_backend, test_app,
};

// ============================================================================
// Fixtures & Helpers
// ============================================================================

#[fixture]
async fn db(
	test_app: (APIClient, ResolvedUrls),
	session_backend: Arc<dyn AsyncSessionBackend>,
) -> (
	ContainerAsync<GenericImage>,
	Arc<DatabaseConnection>,
	APIClient,
	ResolvedUrls,
	Arc<dyn AsyncSessionBackend>,
) {
	let (client, urls) = test_app;
	let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
	let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
		.await
		.expect("Failed to start PostgreSQL with migrations");
	(container, conn, client, urls, session_backend)
}

async fn create_cluster(client: &APIClient, org_slug: &str, name: &str) -> i64 {
	let data = json!({
		"name": name,
		"api_url": format!("https://{name}.k8s.local:6443")
	});
	let resp = client
		.post(&format!("/api/orgs/{org_slug}/clusters/"), &data, "json")
		.await
		.expect("Create cluster failed");
	assert_eq!(resp.status_code(), 201);
	let body: serde_json::Value = resp.json().expect("parse cluster response");
	body["id"].as_i64().expect("Cluster id should be i64")
}

async fn create_deployment(
	client: &APIClient,
	org_slug: &str,
	app_name: &str,
	cluster_id: i64,
) -> i64 {
	let data = json!({
		"app_name": app_name,
		"cluster_id": cluster_id,
		"image": format!("registry.example.com/{app_name}:v1")
	});
	let resp = client
		.post(&format!("/api/orgs/{org_slug}/deployments/"), &data, "json")
		.await
		.expect("Create deployment failed");
	assert_eq!(resp.status_code(), 201);
	let body: serde_json::Value = resp.json().expect("parse deployment response");
	body["id"].as_i64().expect("Deployment id should be i64")
}

// ============================================================================
// Tests
// ============================================================================

/// Two users with different resource counts see only their own data.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[serial(database)]
async fn test_two_users_full_isolation(
	#[future] db: (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		ResolvedUrls,
		Arc<dyn AsyncSessionBackend>,
	),
	test_app: (APIClient, ResolvedUrls),
) {
	// Arrange
	let (_container, conn, client, _urls, backend) = db.await;

	// --- User A: 2 clusters, 2 deployments ---
	let (_user_a, org_a) =
		force_login_user_with_org(&client, &conn, &backend, "iso_user_a", "iso_a@example.com")
			.await;
	let slug_a = &org_a.slug;

	let cluster_a1 = create_cluster(&client, slug_a, "iso-cluster-a1").await;
	let cluster_a2 = create_cluster(&client, slug_a, "iso-cluster-a2").await;
	create_deployment(&client, slug_a, "app-a1", cluster_a1).await;
	create_deployment(&client, slug_a, "app-a2", cluster_a2).await;

	// --- User B: 1 cluster, 1 deployment ---
	let (client_b, _) = test_app;
	let (_user_b, org_b) = force_login_user_with_org(
		&client_b,
		&conn,
		&backend,
		"iso_user_b",
		"iso_b@example.com",
	)
	.await;
	let slug_b = &org_b.slug;

	let cluster_b1 = create_cluster(&client_b, slug_b, "iso-cluster-b1").await;
	create_deployment(&client_b, slug_b, "app-b1", cluster_b1).await;

	// Assert -- User A sees exactly 2 clusters and 2 deployments
	let clusters_a = client
		.get(&format!("/api/orgs/{slug_a}/clusters/"))
		.await
		.expect("List clusters A failed");
	assert_eq!(clusters_a.status_code(), 200);
	let ca_body: serde_json::Value = clusters_a.json().expect("parse clusters A");
	let ca_items = ca_body["items"].as_array().expect("items array");
	assert_eq!(ca_items.len(), 2, "User A should have exactly 2 clusters");
	let ca_names: Vec<&str> = ca_items
		.iter()
		.map(|c| c["name"].as_str().unwrap())
		.collect();
	assert!(ca_names.contains(&"iso-cluster-a1"));
	assert!(ca_names.contains(&"iso-cluster-a2"));

	let deps_a = client
		.get(&format!("/api/orgs/{slug_a}/deployments/"))
		.await
		.expect("List deployments A failed");
	assert_eq!(deps_a.status_code(), 200);
	let da_body: serde_json::Value = deps_a.json().expect("parse deployments A");
	let da_items = da_body["items"].as_array().expect("items array");
	assert_eq!(
		da_items.len(),
		2,
		"User A should have exactly 2 deployments"
	);

	// Assert -- User B sees exactly 1 cluster and 1 deployment
	let clusters_b = client_b
		.get(&format!("/api/orgs/{slug_b}/clusters/"))
		.await
		.expect("List clusters B failed");
	assert_eq!(clusters_b.status_code(), 200);
	let cb_body: serde_json::Value = clusters_b.json().expect("parse clusters B");
	let cb_items = cb_body["items"].as_array().expect("items array");
	assert_eq!(cb_items.len(), 1, "User B should have exactly 1 cluster");
	assert_eq!(cb_items[0]["name"], "iso-cluster-b1");

	let deps_b = client_b
		.get(&format!("/api/orgs/{slug_b}/deployments/"))
		.await
		.expect("List deployments B failed");
	assert_eq!(deps_b.status_code(), 200);
	let db_body: serde_json::Value = deps_b.json().expect("parse deployments B");
	let db_items = db_body["items"].as_array().expect("items array");
	assert_eq!(db_items.len(), 1, "User B should have exactly 1 deployment");
	assert_eq!(db_items[0]["app_name"], "app-b1");
}

/// Multiple deployments on the same cluster are all listed correctly.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[serial(database)]
async fn test_multiple_deployments_same_cluster(
	#[future] db: (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		ResolvedUrls,
		Arc<dyn AsyncSessionBackend>,
	),
) {
	// Arrange
	let (_container, conn, client, _urls, backend) = db.await;
	let (_user, org) = force_login_user_with_org(
		&client,
		&conn,
		&backend,
		"multi_deploy_user",
		"multi@example.com",
	)
	.await;
	let slug = &org.slug;

	let cluster_id = create_cluster(&client, slug, "multi-deploy-cluster").await;

	// Act -- create 3 deployments on the same cluster
	create_deployment(&client, slug, "svc-web", cluster_id).await;
	create_deployment(&client, slug, "svc-api", cluster_id).await;
	create_deployment(&client, slug, "svc-worker", cluster_id).await;

	// Assert
	let list_resp = client
		.get(&format!("/api/orgs/{slug}/deployments/"))
		.await
		.expect("List deployments failed");
	assert_eq!(list_resp.status_code(), 200);
	let body: serde_json::Value = list_resp.json().expect("parse list response");
	let items = body["items"].as_array().expect("items array");
	assert_eq!(items.len(), 3, "Expected 3 deployments");

	// All deployments should reference the same cluster_id
	for item in items {
		assert_eq!(
			item["cluster_id"].as_i64().unwrap(),
			cluster_id,
			"All deployments should belong to cluster {cluster_id}"
		);
	}

	let app_names: Vec<&str> = items
		.iter()
		.map(|d| d["app_name"].as_str().unwrap())
		.collect();
	assert!(app_names.contains(&"svc-web"));
	assert!(app_names.contains(&"svc-api"));
	assert!(app_names.contains(&"svc-worker"));
}
