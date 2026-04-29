//! End-to-end tests for clusters API endpoints.

#[cfg(test)]
mod tests {
	use reinhardt::middleware::session::AsyncSessionBackend;
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::test::APIClient;
	use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage};
	use rstest::*;
	use serde_json::json;
	use serial_test::serial;
	use std::sync::Arc;

	use crate::config::test_helpers::{
		ResolvedUrls, force_login_user_with_org, session_backend, test_app,
	};

	#[fixture]
	async fn db(
		session_backend: Arc<dyn AsyncSessionBackend>,
	) -> (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		ResolvedUrls,
		Arc<dyn AsyncSessionBackend>,
	) {
		// Start the TestContainers database first so that build_test_app() can
		// register the DatabaseConnection in the DI singleton scope. This ensures
		// view handlers that inject Depends<DatabaseConnection> see the same DB
		// as helpers using create_with_conn. Fixes #459.
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations");
		let (client, urls) = crate::config::test_helpers::build_test_app();
		(container, conn, client, urls, session_backend)
	}

	/// Verify unauthenticated GET /api/orgs/{org}/clusters/ returns 401.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_unauthenticated_clusters_returns_401(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
			Arc<dyn AsyncSessionBackend>,
		),
	) {
		// Arrange
		let (_container, _conn, client, _urls, _backend) = db.await;

		// Act -- use a placeholder slug; the auth middleware rejects before routing
		let response = client
			.get("/api/orgs/my-org/clusters/")
			.await
			.expect("List clusters request failed");

		// Assert
		assert_eq!(response.status_code(), 401);
	}

	/// Verify GET /api/orgs/{org}/clusters/ returns empty list when authenticated.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_list_clusters_empty(
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
		let (_user, org) =
			force_login_user_with_org(&client, &conn, &backend, "testuser", "test@example.com")
				.await;
		let slug = &org.slug;

		// Act
		let response = client
			.get(&format!("/api/orgs/{slug}/clusters/"))
			.await
			.expect("List clusters request failed");

		// Assert
		assert_eq!(response.status_code(), 200);
		let body: serde_json::Value = response.json().expect("Failed to parse JSON response");
		assert_eq!(body["items"], json!([]));
		assert_eq!(body["total"], 0);
		assert!(body["page"].is_number());
		assert!(body["page_size"].is_number());
	}

	/// Verify POST /api/orgs/{org}/clusters/ creates a cluster, then GET returns it.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_create_cluster_persists(
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
		let (_user, org) =
			force_login_user_with_org(&client, &conn, &backend, "testuser", "test@example.com")
				.await;
		let slug = &org.slug;

		let cluster_data = json!({
			"name": "production-cluster",
			"api_url": "https://k8s.example.com:6443"
		});

		// Act -- create cluster
		let create_response = client
			.post(
				&format!("/api/orgs/{slug}/clusters/"),
				&cluster_data,
				"json",
			)
			.await
			.expect("Create cluster request failed");

		// Assert -- creation response
		assert_eq!(create_response.status_code(), 201);
		let created: serde_json::Value = create_response
			.json()
			.expect("Failed to parse create response");
		assert_eq!(created["name"], "production-cluster");
		assert_eq!(created["api_url"], "https://k8s.example.com:6443");
		assert_eq!(created["is_active"], true);
		assert!(created["id"].is_number());

		// Act -- list clusters to verify persistence
		let list_response = client
			.get(&format!("/api/orgs/{slug}/clusters/"))
			.await
			.expect("List clusters request failed");

		// Assert -- cluster appears in list
		assert_eq!(list_response.status_code(), 200);
		let body: serde_json::Value = list_response.json().expect("Failed to parse list response");
		let items = body["items"].as_array().expect("items should be an array");
		assert_eq!(items.len(), 1);
		assert_eq!(items[0]["name"], "production-cluster");
		assert_eq!(items[0]["api_url"], "https://k8s.example.com:6443");
		assert_eq!(body["total"], 1);
		assert!(body["page"].is_number());
		assert!(body["page_size"].is_number());
	}

	/// Verify creating a second cluster with the same name in the same
	/// organization is rejected with 409 Conflict (refs #436).
	///
	/// The uniqueness check is enforced at the application layer as a
	/// workaround for `kent8192/reinhardt-web#3989` (tracking issue
	/// #443) — the framework's `makemigrations` does not currently emit
	/// `AlterUniqueTogether` for `unique_together` declared on existing
	/// tables, so the DB constraint is not yet in place. This test will
	/// remain valid once that workaround is removed and replaced by the
	/// real DB constraint.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_create_cluster_duplicate_name_returns_conflict(
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
		let (_user, org) =
			force_login_user_with_org(&client, &conn, &backend, "testuser", "test@example.com")
				.await;
		let slug = &org.slug;

		let cluster_data = json!({
			"name": "shared-name",
			"api_url": "https://k8s.example.com:6443"
		});

		// Act -- first create succeeds
		let first = client
			.post(
				&format!("/api/orgs/{slug}/clusters/"),
				&cluster_data,
				"json",
			)
			.await
			.expect("First create cluster request failed");
		assert_eq!(first.status_code(), 201);

		// Act -- second create with the same name in the same org is rejected
		let duplicate_data = json!({
			"name": "shared-name",
			"api_url": "https://k8s2.example.com:6443"
		});
		let second = client
			.post(
				&format!("/api/orgs/{slug}/clusters/"),
				&duplicate_data,
				"json",
			)
			.await
			.expect("Second create cluster request failed");

		// Assert -- 409 Conflict, name-collision-specific message
		assert_eq!(second.status_code(), 409);
		let body: serde_json::Value = second.json().expect("Failed to parse JSON response");
		assert_eq!(body["error"], "Conflict");
		assert_eq!(
			body["detail"], "Cluster name already exists in this organization",
			"Conflict response must surface the cluster-name-collision message"
		);
	}

	/// Verify renaming a cluster to a name already taken by another cluster
	/// in the same organization returns 409 Conflict (refs #436).
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_update_cluster_to_taken_name_returns_conflict(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
			Arc<dyn AsyncSessionBackend>,
		),
	) {
		// Arrange -- create two clusters with distinct names
		let (_container, conn, client, _urls, backend) = db.await;
		let (_user, org) =
			force_login_user_with_org(&client, &conn, &backend, "testuser", "test@example.com")
				.await;
		let slug = &org.slug;

		let first_response = client
			.post(
				&format!("/api/orgs/{slug}/clusters/"),
				&json!({
					"name": "alpha",
					"api_url": "https://alpha.example.com:6443"
				}),
				"json",
			)
			.await
			.expect("Create alpha request failed");
		assert_eq!(first_response.status_code(), 201);

		let second_response = client
			.post(
				&format!("/api/orgs/{slug}/clusters/"),
				&json!({
					"name": "beta",
					"api_url": "https://beta.example.com:6443"
				}),
				"json",
			)
			.await
			.expect("Create beta request failed");
		assert_eq!(second_response.status_code(), 201);
		let beta: serde_json::Value = second_response.json().expect("parse beta response");
		let beta_id = beta["id"].as_i64().expect("beta id is i64");

		// Act -- rename beta to alpha (already taken in this org)
		let conflict_response = client
			.patch(
				&format!("/api/orgs/{slug}/clusters/{beta_id}/"),
				&json!({ "name": "alpha" }),
				"json",
			)
			.await
			.expect("PATCH beta -> alpha request failed");

		// Assert -- 409 Conflict
		assert_eq!(conflict_response.status_code(), 409);
		let body: serde_json::Value = conflict_response
			.json()
			.expect("Failed to parse JSON response");
		assert_eq!(body["error"], "Conflict");
		assert_eq!(
			body["detail"],
			"Cluster name already exists in this organization",
		);
	}

	/// Cross-organization duplicates MUST be allowed — the unique constraint
	/// is scoped to `(organization_id, name)`, not to `name` alone (refs
	/// #436). Two different organizations may each own a cluster called
	/// `prod` without any collision.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_cross_organization_same_name_succeeds(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
			Arc<dyn AsyncSessionBackend>,
		),
	) {
		// Arrange -- two distinct users, each in their own Personal Org
		let (_container, conn, client, _urls, backend) = db.await;

		let cluster_data = json!({
			"name": "prod",
			"api_url": "https://k8s.example.com:6443"
		});

		// Act -- user A in org A creates cluster "prod"
		let (_user_a, org_a) =
			force_login_user_with_org(&client, &conn, &backend, "user_a", "a@example.com").await;
		let slug_a = &org_a.slug;
		let resp_a = client
			.post(
				&format!("/api/orgs/{slug_a}/clusters/"),
				&cluster_data,
				"json",
			)
			.await
			.expect("Create cluster (user A) request failed");

		// Act -- user B in org B creates cluster "prod"
		let (_user_b, org_b) =
			force_login_user_with_org(&client, &conn, &backend, "user_b", "b@example.com").await;
		let slug_b = &org_b.slug;
		let resp_b = client
			.post(
				&format!("/api/orgs/{slug_b}/clusters/"),
				&cluster_data,
				"json",
			)
			.await
			.expect("Create cluster (user B) request failed");

		// Assert -- both succeed; uniqueness is per-organization, not global
		assert_eq!(
			resp_a.status_code(),
			201,
			"User A's cluster should be created"
		);
		assert_eq!(
			resp_b.status_code(),
			201,
			"User B's cluster should also be created — same name, different org"
		);
	}

	/// Verify renaming a cluster to a different (unused) name still succeeds
	/// — guards against the pre-check workaround over-rejecting (refs #436).
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_update_cluster_to_unused_name_succeeds(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
			Arc<dyn AsyncSessionBackend>,
		),
	) {
		// Arrange -- one cluster
		let (_container, conn, client, _urls, backend) = db.await;
		let (_user, org) =
			force_login_user_with_org(&client, &conn, &backend, "testuser", "test@example.com")
				.await;
		let slug = &org.slug;

		let create = client
			.post(
				&format!("/api/orgs/{slug}/clusters/"),
				&json!({
					"name": "original",
					"api_url": "https://original.example.com:6443"
				}),
				"json",
			)
			.await
			.expect("Create cluster request failed");
		assert_eq!(create.status_code(), 201);
		let created: serde_json::Value = create.json().expect("parse create response");
		let cluster_id = created["id"].as_i64().expect("cluster id is i64");

		// Act -- rename to a free name
		let response = client
			.patch(
				&format!("/api/orgs/{slug}/clusters/{cluster_id}/"),
				&json!({ "name": "renamed" }),
				"json",
			)
			.await
			.expect("PATCH rename request failed");

		// Assert -- 200 OK with new name
		assert_eq!(response.status_code(), 200);
		let body: serde_json::Value = response.json().expect("Failed to parse JSON response");
		assert_eq!(body["name"], "renamed");
	}
}
