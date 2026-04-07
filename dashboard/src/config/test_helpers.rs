//! Test helpers for dashboard end-to-end tests.
//!
//! Provides [`TestAppGuard`] which spawns the dashboard HTTP server on a
//! random port with `AllowedOrigins` pre-registered in the `SingletonScope`
//! so the `OriginGuardMiddleware` accepts requests from the test client.
//!
//! Uses reinhardt-web's public DI APIs (`SingletonScope::set`,
//! `InjectionContext::get_singleton`).
//!
//! All singletons (`WsBroadcaster`, `LocalAuthService`, etc.) can be
//! overridden by pre-registering in the scope before calling `build_routes`.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use reinhardt::Handler;
use reinhardt::di::SingletonScope;
use reinhardt::server::{HttpServer, ShutdownCoordinator};
use reinhardt::test::APIClient;
use reinhardt::test::fixtures::api_client_from_url;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use crate::config::urls::{AllowedOrigins, build_routes};

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

/// Spawn a test server with `AllowedOrigins` injected via DI.
///
/// 1. Binds a `TcpListener` to a random port
/// 2. Registers `AllowedOrigins` in `SingletonScope` with the test server URL
/// 3. Builds the router via `routes(scope)` — OriginGuard reads from DI
/// 4. Spawns the HTTP server on the pre-bound listener
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

	// Register AllowedOrigins in DI scope — routes() reads this via
	// get_singleton::<AllowedOrigins>() during OriginGuard construction.
	let scope = Arc::new(SingletonScope::new());
	scope.set(AllowedOrigins(vec![url.clone()]));
	let router = build_routes(scope).into_server();

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
