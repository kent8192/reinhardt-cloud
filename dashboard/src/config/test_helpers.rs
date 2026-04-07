//! Test helpers for dashboard end-to-end tests.
//!
//! Provides [`TestAppGuard`] which spawns the dashboard HTTP server on a
//! random port and builds the router with that port's origin already in
//! the `OriginGuardMiddleware` allow-list.  This avoids the 403 errors
//! caused by the test server binding to a random port that doesn't match
//! the default allowed origins (`localhost:8000` / `127.0.0.1:8000`).
//!
//! The helper replicates the minimal server-spawning logic from
//! `reinhardt-testkit` so the listener, router, and origin configuration
//! can be wired together atomically — without a TOCTOU race on port
//! numbers.
//!
//! Workaround for kent8192/reinhardt-web#3375 (tracked in reinhardt-cloud#297)
//! Remove this entire module when `CoreSettings.allowed_origins` + DI-based
//! configuration override is available. The ideal approach is to register
//! `AllowedOrigins` in `SingletonScope` and use `InjectionContext.get_singleton()`
//! during router construction, with tests overriding via `singleton.set()`.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use reinhardt::Handler;
use reinhardt::server::{HttpServer, ShutdownCoordinator};
use reinhardt::test::APIClient;
use reinhardt::test::fixtures::api_client_from_url;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use crate::config::urls::routes_with_origins;

/// Guard that owns a running test server and shuts it down on drop.
///
/// Created via [`test_app_with_origin_guard`].  The guard keeps the
/// server task alive until it is dropped, at which point the
/// `ShutdownCoordinator` signal is sent and the task is aborted.
pub struct TestAppGuard {
	/// Base URL of the running server (e.g. `http://127.0.0.1:54321`).
	pub url: String,
	/// Shutdown coordinator — triggers graceful shutdown on drop.
	coordinator: Arc<ShutdownCoordinator>,
	/// Handle to the spawned server task.
	server_task: Option<JoinHandle<()>>,
}

impl Drop for TestAppGuard {
	fn drop(&mut self) {
		self.coordinator.shutdown();
		if let Some(task) = self.server_task.take() {
			task.abort();
		}
	}
}

/// Maximum number of TCP readiness probe attempts.
const SERVER_READY_MAX_ATTEMPTS: u32 = 20;

/// Interval between TCP readiness probe attempts.
const SERVER_READY_PROBE_INTERVAL_MS: u64 = 50;

/// Probe the server address until it accepts a TCP connection.
async fn wait_for_server_ready(addr: SocketAddr) -> Result<(), std::io::Error> {
	for attempt in 1..=SERVER_READY_MAX_ATTEMPTS {
		match tokio::net::TcpStream::connect(addr).await {
			Ok(_) => return Ok(()),
			Err(_) if attempt < SERVER_READY_MAX_ATTEMPTS => {
				tokio::time::sleep(Duration::from_millis(SERVER_READY_PROBE_INTERVAL_MS)).await;
			}
			Err(e) => {
				return Err(std::io::Error::new(
					std::io::ErrorKind::TimedOut,
					format!(
						"Server at {} not ready after {} attempts: {}",
						addr, SERVER_READY_MAX_ATTEMPTS, e
					),
				));
			}
		}
	}

	Err(std::io::Error::new(
		std::io::ErrorKind::TimedOut,
		format!(
			"Server at {} not ready after {} attempts",
			addr, SERVER_READY_MAX_ATTEMPTS
		),
	))
}

/// Spawn a test server whose `OriginGuardMiddleware` already allows the
/// random-port origin.
///
/// Returns `(guard, client)` where:
/// - `guard` keeps the server alive and shuts it down on drop
/// - `client` is an [`APIClient`] with `base_url` pointing at the server
///
/// # Panics
///
/// Panics if the TCP listener cannot bind or the server fails to become
/// ready within the probe window.
pub async fn test_app_with_origin_guard() -> (TestAppGuard, APIClient) {
	let shutdown_timeout = Duration::from_secs(5);

	// Bind to a random port and keep the listener to avoid TOCTOU race.
	let listener = TcpListener::bind("127.0.0.1:0")
		.await
		.expect("Failed to bind TcpListener to 127.0.0.1:0");
	let actual_addr = listener.local_addr().expect("Failed to get local addr");
	let url = format!("http://{}", actual_addr);

	// Build the router with this exact origin in the allow-list so that
	// the OriginGuardMiddleware accepts requests from the test client.
	let extra = vec![url.clone()];
	let router = routes_with_origins(&extra).into_server();

	// Create shutdown coordinator.
	let coordinator = Arc::new(ShutdownCoordinator::new(shutdown_timeout));

	// Spawn the HTTP server using the already-bound listener.
	let server_coordinator = (*coordinator).clone();
	let handler: Arc<dyn Handler> = Arc::new(router);
	let server = HttpServer::new(handler);
	let mut shutdown_rx = server_coordinator.subscribe();
	let server_task = tokio::spawn(async move {
		loop {
			tokio::select! {
				result = listener.accept() => {
					match result {
						Ok((stream, socket_addr)) => {
							let handler_clone = server.handler();
							tokio::spawn(async move {
								if let Err(e) =
									HttpServer::handle_connection(
										stream,
										socket_addr,
										handler_clone,
										None,
									)
									.await
								{
									eprintln!("Error handling connection: {:?}", e);
								}
							});
						}
						Err(e) => {
							eprintln!("Error accepting connection: {:?}", e);
							break;
						}
					}
				}
				_ = shutdown_rx.recv() => {
					break;
				}
			}
		}
	});

	// Wait for the server to be ready.
	wait_for_server_ready(actual_addr)
		.await
		.expect("Test server failed to become ready");

	let guard = TestAppGuard {
		url: url.clone(),
		coordinator,
		server_task: Some(server_task),
	};

	let client = api_client_from_url(&url);
	(guard, client)
}
