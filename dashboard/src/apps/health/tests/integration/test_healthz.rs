//! Integration tests for the `/api/healthz/` endpoint.
//!
//! These tests exercise the router end-to-end:
//!
//! - The unauthenticated client is accepted (the auth middleware skips
//!   `/api/healthz/`).
//! - A healthy database yields `"db": "ok"`.
//! - A broken database probe (pointed at a non-existent Postgres) yields
//!   `"db": "error"` and HTTP 503.
//!
//! The gRPC probe is not asserted to succeed — no gRPC server is run
//! inside this test process, so the probe will report `"grpc": "error"`
//! and the overall status will also be `"error"`. The DB-only positive
//! assertion therefore targets the `db` field specifically.

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use reinhardt::di::{InjectionContext, SingletonScope};
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::test::APIClient;
	use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage};
	use rstest::*;
	use serial_test::serial;

	use crate::apps::health::services::GrpcChannelSingleton;
	use crate::config::test_helpers::{ResolvedUrls, test_app};

	/// gRPC endpoint used by test probes.
	///
	/// Points at an unroutable port so the gRPC probe deterministically
	/// reports `error` within the 2-second probe timeout. Using a real
	/// endpoint from production settings is unsafe in CI.
	const TEST_GRPC_ENDPOINT: &str = "http://127.0.0.1:1";

	#[fixture]
	async fn db_with_app(
		test_app: (APIClient, ResolvedUrls),
	) -> (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		ResolvedUrls,
	) {
		let (client, urls) = test_app;
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations");
		(container, conn, client, urls)
	}

	/// Verify GET /api/healthz/ succeeds when the database is reachable.
	///
	/// The gRPC probe is expected to fail (no gRPC server is running in
	/// this unit-test process), so the overall `status` and HTTP code
	/// reflect a degraded response; only the `db` probe is asserted to
	/// be `"ok"`. This still proves the endpoint bypasses auth and the
	/// DB probe exercises the live connection end-to-end.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_healthz_returns_200_when_healthy(
		#[future] db_with_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
		),
	) {
		// Arrange
		let (_container, _conn, client, _urls) = db_with_app.await;

		// Act
		let response = client
			.get("/api/healthz/")
			.await
			.expect("healthz request failed");

		// Assert
		let status = response.status_code();
		let body: serde_json::Value = response.json().expect("Failed to parse JSON response");
		assert_eq!(
			body["db"], "ok",
			"DB probe should succeed against live postgres; body={body}"
		);
		// Healthy DB but unreachable gRPC => 503, grpc=error, status=error.
		assert_eq!(status, 503);
		assert_eq!(body["grpc"], "error");
		assert_eq!(body["status"], "error");
	}

	/// Verify GET /api/healthz/ returns 503 when the database probe fails.
	///
	/// Uses a custom DI context that does NOT initialise the global
	/// database connection (no `postgres_with_migrations_from_dir`
	/// fixture), so `User::objects().count()` fails with a connection
	/// error and the probe reports `"db": "error"`.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_healthz_returns_503_when_db_down() {
		// Arrange -- build a client without starting postgres.
		use crate::config::urls::{AllowedOrigins, DashboardRouter};
		use reinhardt::OpenApiRouter;

		let scope = Arc::new(SingletonScope::new());
		scope.set(AllowedOrigins(vec!["http://testserver".into()]));
		// Pre-register a gRPC singleton pointing at an unroutable endpoint so
		// the probe fails fast regardless of the ambient `GRPC_ENDPOINT`.
		scope.set(
			GrpcChannelSingleton::new(TEST_GRPC_ENDPOINT)
				.expect("Failed to build test gRPC channel singleton"),
		);
		let di_ctx = Arc::new(InjectionContext::builder(scope).build());

		let router: Arc<DashboardRouter> = tokio::task::block_in_place(|| {
			tokio::runtime::Handle::current().block_on(di_ctx.resolve::<DashboardRouter>())
		})
		.expect("Failed to resolve DashboardRouter");
		let server_router = Arc::new(
			Arc::try_unwrap(router)
				.expect("DashboardRouter has multiple owners after resolve")
				.0
				.into_server(),
		);
		let handler =
			OpenApiRouter::wrap(server_router).expect("Failed to wrap with OpenApiRouter");
		let client = APIClient::from_handler(handler);

		// Act
		let response = client
			.get("/api/healthz/")
			.await
			.expect("healthz request failed");

		// Assert
		let status = response.status_code();
		let body: serde_json::Value = response.json().expect("Failed to parse JSON response");
		assert_eq!(status, 503, "healthz must degrade to 503 when DB is down; body={body}");
		assert_eq!(body["status"], "error");
		assert_eq!(body["db"], "error");
	}
}
