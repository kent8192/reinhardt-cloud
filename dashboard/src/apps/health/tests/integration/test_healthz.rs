//! Integration tests for the `/api/healthz/` endpoint.
//!
//! These tests exercise the router end-to-end:
//!
//! - The unauthenticated client is accepted (the auth middleware skips
//!   `/api/healthz/`).
//! - All probes succeeding (live Postgres + in-process gRPC server with
//!   `Health/Check` returning `SERVING`) yields HTTP 200 and
//!   `"status": "ok"`.
//! - A reachable database paired with an unreachable gRPC endpoint
//!   yields HTTP 503 and `"grpc": "error"`.
//! - A broken database probe (pointed at a non-existent Postgres) yields
//!   `"db": "error"` and HTTP 503.

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use reinhardt::di::{FactoryOutput, InjectionContext, SingletonScope};
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::test::APIClient;
	use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage};
	use rstest::*;
	use serial_test::serial;

	use crate::config::{GrpcChannelSingleton, GrpcChannelSingletonKey};
	use reinhardt::UrlReverser;

	/// gRPC endpoint used by test probes.
	///
	/// Points at an unroutable port so the gRPC probe deterministically
	/// reports `error` within the 2-second probe timeout. Using a real
	/// endpoint from production settings is unsafe in CI.
	const TEST_GRPC_ENDPOINT: &str = "http://127.0.0.1:1";

	#[fixture]
	async fn db_with_app() -> (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		Arc<UrlReverser>,
	) {
		// Start TestContainers first so build_test_app() registers DatabaseConnection
		// in the DI scope. Fixes #459.
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations");
		let (client, urls) = crate::config::test_helpers::build_test_app();
		(container, conn, client, urls)
	}

	/// Verify GET /api/healthz/ degrades to 503 when gRPC is unreachable.
	///
	/// Postgres is reachable so `db` reports `"ok"`, but no gRPC server
	/// is started inside this test process — the probe therefore reports
	/// `"grpc": "error"` and the endpoint returns HTTP 503 with overall
	/// `"status": "error"`. This proves both the auth bypass and the
	/// live DB probe end-to-end while exercising the degraded path.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_healthz_returns_503_when_grpc_unreachable(
		#[future] db_with_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<UrlReverser>,
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

	/// Verify GET /api/healthz/ returns 200 when both probes succeed.
	///
	/// Brings up an in-process gRPC server via `TestGrpcServer` whose
	/// default `Health/Check` returns `SERVING` for the empty service
	/// name (matching the probe in `server_urls::probe_grpc`). The
	/// `GrpcChannelSingleton` is pre-registered in the DI scope so the
	/// view's RPC targets the test server. With live Postgres + serving
	/// gRPC, the response must be HTTP 200 with `"status": "ok"`.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_healthz_returns_200_when_all_probes_succeed(
		#[future] db_with_app: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<UrlReverser>,
		),
	) {
		use crate::config::urls::{
			AllowedOrigins, AllowedOriginsKey, DashboardRouter, DashboardRouterKey,
		};
		use reinhardt::OpenApiRouter;
		use reinhardt_cloud_grpc::test_utils::TestGrpcServer;

		// Arrange — bring up live Postgres (via fixture) and an
		// in-process gRPC health server, then build a router using a
		// custom DI scope that points the channel singleton at the
		// test server's bound port.
		let (_container, _conn, _client, _urls) = db_with_app.await;
		let grpc_server = TestGrpcServer::start().await;
		let endpoint = grpc_server.endpoint();

		let scope = Arc::new(SingletonScope::new());
		scope.set(FactoryOutput::<AllowedOriginsKey, AllowedOrigins>::new(
			AllowedOrigins(vec!["http://testserver".into()]),
		));
		scope.set(
			FactoryOutput::<GrpcChannelSingletonKey, GrpcChannelSingleton>::new(
				GrpcChannelSingleton::new(&endpoint)
					.expect("Failed to build test gRPC channel singleton"),
			),
		);
		let di_ctx = Arc::new(InjectionContext::builder(scope).build());

		let router: Arc<FactoryOutput<DashboardRouterKey, DashboardRouter>> =
			tokio::task::block_in_place(|| {
				tokio::runtime::Handle::current().block_on(
					di_ctx.resolve::<FactoryOutput<DashboardRouterKey, DashboardRouter>>(),
				)
			})
			.expect("Failed to resolve DashboardRouter");
		let server_router = Arc::new(
			Arc::try_unwrap(router)
				.expect("DashboardRouter has multiple owners after resolve")
				.into_inner()
				.0
				.into_server(),
		);
		let handler =
			OpenApiRouter::wrap(server_router).expect("Failed to wrap with OpenApiRouter");
		let probe_client = APIClient::from_handler(handler);

		// Act
		let response = probe_client
			.get("/api/healthz/")
			.await
			.expect("healthz request failed");

		// Assert
		let status = response.status_code();
		let body: serde_json::Value = response.json().expect("Failed to parse JSON response");
		assert_eq!(
			status, 200,
			"healthz must return 200 when DB and gRPC are both healthy; body={body}"
		);
		assert_eq!(body["status"], "ok");
		assert_eq!(body["db"], "ok");
		assert_eq!(body["grpc"], "ok");

		// Cleanup
		grpc_server.shutdown().await;
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
		use crate::config::urls::{
			AllowedOrigins, AllowedOriginsKey, DashboardRouter, DashboardRouterKey,
		};
		use reinhardt::OpenApiRouter;

		let scope = Arc::new(SingletonScope::new());
		scope.set(FactoryOutput::<AllowedOriginsKey, AllowedOrigins>::new(
			AllowedOrigins(vec!["http://testserver".into()]),
		));
		// Pre-register a gRPC singleton pointing at an unroutable endpoint so
		// the probe fails fast regardless of the ambient `GRPC_ENDPOINT`.
		scope.set(
			FactoryOutput::<GrpcChannelSingletonKey, GrpcChannelSingleton>::new(
				GrpcChannelSingleton::new(TEST_GRPC_ENDPOINT)
					.expect("Failed to build test gRPC channel singleton"),
			),
		);
		let di_ctx = Arc::new(InjectionContext::builder(scope).build());

		let router: Arc<FactoryOutput<DashboardRouterKey, DashboardRouter>> =
			tokio::task::block_in_place(|| {
				tokio::runtime::Handle::current().block_on(
					di_ctx.resolve::<FactoryOutput<DashboardRouterKey, DashboardRouter>>(),
				)
			})
			.expect("Failed to resolve DashboardRouter");
		let server_router = Arc::new(
			Arc::try_unwrap(router)
				.expect("DashboardRouter has multiple owners after resolve")
				.into_inner()
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
		assert_eq!(
			status, 503,
			"healthz must degrade to 503 when DB is down; body={body}"
		);
		assert_eq!(body["status"], "error");
		assert_eq!(body["db"], "error");
	}
}
