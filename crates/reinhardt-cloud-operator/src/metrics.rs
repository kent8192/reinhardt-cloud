//! Prometheus metrics for the reinhardt-cloud operator.
//!
//! Metrics are registered in a dedicated `prometheus::Registry` so that
//! the operator's gauges/counters are exposed on a private `/metrics`
//! endpoint without polluting the global default registry.
//!
//! ## Metrics
//!
//! - `reinhardt_cloud_operator_reconcile_total{result}` — total number of
//!   reconciliation attempts labeled by result (`success` or error class).
//! - `reinhardt_cloud_operator_reconcile_duration_seconds{result}` —
//!   histogram of reconciliation durations.
//! - `reinhardt_cloud_operator_requeue_total{reason}` — number of requeues
//!   issued by the error policy, labeled by backoff class. Only branches
//!   that actually return `Action::requeue(...)` increment this counter;
//!   `Permanent` errors return `Action::await_change()` and do not bump
//!   the counter.
//! - `reinhardt_cloud_operator_managed_apps{phase}` — gauge of `ReinhardtApp`
//!   objects currently tracked by the reconciler, labeled by phase.
//!   Incremented when a new phase is observed during status update and
//!   decremented when the object is cleaned up or transitions to a
//!   different phase.

use std::sync::Arc;
use std::time::Duration;

use prometheus::{
	CounterVec, Encoder, GaugeVec, HistogramVec, Opts, Registry, TextEncoder, histogram_opts,
};
use tokio::sync::Semaphore;

/// Maximum number of concurrent `/metrics` connections served by the
/// exporter. Prometheus scrapes are serialized per target, so a small cap
/// is sufficient and prevents slowloris-style FD/task exhaustion.
const MAX_CONCURRENT_METRICS_CONNECTIONS: usize = 32;

/// Upper bound on how long the exporter waits for a single request-read
/// or response-write step before giving up.
const METRICS_IO_TIMEOUT: Duration = Duration::from_secs(5);

/// Whether the HTTP server should expose Prometheus output on `/metrics`.
///
/// `/healthz` is served unconditionally so kubelet probes always succeed
/// while the process is running. The `/metrics` endpoint, in contrast,
/// is gated by chart configuration (`metrics.enabled`) so an unmonitored
/// install does not pretend to expose telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetricsMode {
	Enabled,
	Disabled,
}

/// Metrics container shared with the reconciler via the `Context`.
pub(crate) struct Metrics {
	pub(crate) registry: Registry,
	pub(crate) reconcile_total: CounterVec,
	pub(crate) reconcile_duration: HistogramVec,
	pub(crate) requeue_total: CounterVec,
	pub(crate) managed_apps: GaugeVec,
}

impl Metrics {
	/// Build and register all operator metrics into a fresh registry.
	///
	/// Returns an `Arc` so the same metrics handle can be shared between
	/// the reconciler context and the HTTP exporter task.
	pub(crate) fn new() -> Arc<Self> {
		let registry = Registry::new();

		let reconcile_total = CounterVec::new(
			Opts::new(
				"reinhardt_cloud_operator_reconcile_total",
				"Total number of reconciliation attempts, labeled by result.",
			),
			&["result"],
		)
		.expect("valid counter definition");

		let reconcile_duration = HistogramVec::new(
			histogram_opts!(
				"reinhardt_cloud_operator_reconcile_duration_seconds",
				"Reconciliation duration in seconds, labeled by result.",
				vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0]
			),
			&["result"],
		)
		.expect("valid histogram definition");

		let requeue_total = CounterVec::new(
			Opts::new(
				"reinhardt_cloud_operator_requeue_total",
				"Total number of requeues issued by the error policy, labeled by reason.",
			),
			&["reason"],
		)
		.expect("valid counter definition");

		let managed_apps = GaugeVec::new(
			Opts::new(
				"reinhardt_cloud_operator_managed_apps",
				"Number of ReinhardtApp objects tracked by the reconciler, labeled by phase.",
			),
			&["phase"],
		)
		.expect("valid gauge definition");

		// Register; unwrap is safe because definitions above are static.
		registry
			.register(Box::new(reconcile_total.clone()))
			.expect("metric registration");
		registry
			.register(Box::new(reconcile_duration.clone()))
			.expect("metric registration");
		registry
			.register(Box::new(requeue_total.clone()))
			.expect("metric registration");
		registry
			.register(Box::new(managed_apps.clone()))
			.expect("metric registration");

		Arc::new(Self {
			registry,
			reconcile_total,
			reconcile_duration,
			requeue_total,
			managed_apps,
		})
	}

	/// Encode the current metric families as a Prometheus text exposition.
	pub(crate) fn encode(&self) -> Vec<u8> {
		let mut buf = Vec::new();
		let encoder = TextEncoder::new();
		let families = self.registry.gather();
		// Encoding to a `Vec<u8>` never fails in practice; on the unlikely
		// event of a write error return an empty buffer rather than panic.
		if encoder.encode(&families, &mut buf).is_err() {
			buf.clear();
		}
		buf
	}
}

/// Spawn a minimal HTTP server on the given bind address.
///
/// The server is implemented with a raw `tokio::net::TcpListener` to
/// avoid pulling hyper/axum into the operator binary just for a single
/// endpoint. It always serves `GET /healthz` (for kubelet probes) and
/// conditionally serves `GET /metrics` with the Prometheus text format
/// when `mode` is [`MetricsMode::Enabled`]. Any other request receives
/// a `404` response.
pub(crate) fn spawn_exporter(
	metrics: Arc<Metrics>,
	bind: std::net::SocketAddr,
	mode: MetricsMode,
) -> tokio::task::JoinHandle<()> {
	tokio::spawn(async move {
		let listener = match tokio::net::TcpListener::bind(bind).await {
			Ok(l) => l,
			Err(err) => {
				tracing::error!("Failed to bind operator HTTP server on {bind}: {err}");
				return;
			}
		};
		tracing::info!(
			"Operator HTTP server listening on http://{bind}/healthz (and /metrics when enabled)"
		);

		// Cap concurrent connections to defend against slowloris
		// and runaway task/FD usage. Prometheus scrapes are serialized per
		// target, so this limit does not block legitimate traffic.
		let sem = Arc::new(Semaphore::new(MAX_CONCURRENT_METRICS_CONNECTIONS));

		loop {
			let (socket, _) = match listener.accept().await {
				Ok(s) => s,
				Err(err) => {
					tracing::warn!("operator http listener accept error: {err}");
					continue;
				}
			};
			// Try to acquire a slot without blocking the accept loop. If
			// no slot is available, drop the connection immediately.
			let permit = match Arc::clone(&sem).try_acquire_owned() {
				Ok(p) => p,
				Err(_) => {
					tracing::warn!(
						"operator http server at capacity ({MAX_CONCURRENT_METRICS_CONNECTIONS}), dropping connection"
					);
					drop(socket);
					continue;
				}
			};
			// Disable Nagle so small response buffers flush promptly.
			let _ = socket.set_nodelay(true);
			let metrics = Arc::clone(&metrics);
			tokio::spawn(async move {
				if let Err(err) = handle_connection(socket, metrics, mode).await {
					tracing::debug!("operator http connection closed with error: {err}");
				}
				drop(permit);
			});
		}
	})
}

/// Classification of an HTTP request line, used by `handle_connection` and
/// reused in tests so the path-matching rule lives in exactly one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestKind {
	Metrics,
	Healthz,
	Other,
}

/// Classify an HTTP request line into one of [`RequestKind`].
///
/// Matches the request line exactly so siblings such as `GET /metricsfoo`
/// or `GET /metrics_admin` are NOT served metrics. Accepts either an
/// HTTP-version separator (`GET /metrics HTTP/1.1`) or a query string
/// (`GET /metrics?...`). The `/metrics` path is additionally gated by
/// [`MetricsMode::Enabled`] so an unmonitored install does not advertise
/// telemetry it is not collecting.
fn classify_request(head: &[u8], mode: MetricsMode) -> RequestKind {
	let is_metrics_path = head.starts_with(b"GET /metrics ") || head.starts_with(b"GET /metrics?");
	let is_healthz_path = head.starts_with(b"GET /healthz ") || head.starts_with(b"GET /healthz?");

	if is_metrics_path && mode == MetricsMode::Enabled {
		RequestKind::Metrics
	} else if is_healthz_path {
		RequestKind::Healthz
	} else {
		RequestKind::Other
	}
}

async fn handle_connection(
	mut socket: tokio::net::TcpStream,
	metrics: Arc<Metrics>,
	mode: MetricsMode,
) -> std::io::Result<()> {
	use tokio::io::{AsyncReadExt, AsyncWriteExt};

	// Read a small request header (we only need the request line). The
	// read is bounded by `METRICS_IO_TIMEOUT` so a client that holds the
	// connection open without sending bytes (slowloris) cannot pin a
	// worker task indefinitely.
	let mut buf = [0u8; 1024];
	let n = match tokio::time::timeout(METRICS_IO_TIMEOUT, socket.read(&mut buf)).await {
		Ok(res) => res?,
		Err(_) => {
			// Read timed out; close the connection without replying.
			let _ = socket.shutdown().await;
			return Ok(());
		}
	};
	if n == 0 {
		return Ok(());
	}
	let head = &buf[..n];
	// Reject requests whose request line did not fit in our buffer — the
	// header is intentionally small, and oversized requests are treated
	// as hostile and closed early.
	if n == buf.len() && !head.contains(&b'\n') {
		return Ok(());
	}
	let kind = classify_request(head, mode);

	let write_fut = async {
		match kind {
			RequestKind::Metrics => {
				let body = metrics.encode();
				let header = format!(
					"HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
					body.len()
				);
				socket.write_all(header.as_bytes()).await?;
				socket.write_all(&body).await?;
			}
			RequestKind::Healthz => {
				let body = b"ok";
				let header = format!(
					"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
					body.len()
				);
				socket.write_all(header.as_bytes()).await?;
				socket.write_all(body).await?;
			}
			RequestKind::Other => {
				let msg = b"not found";
				let header = format!(
					"HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
					msg.len()
				);
				socket.write_all(header.as_bytes()).await?;
				socket.write_all(msg).await?;
			}
		}
		socket.shutdown().await?;
		Ok::<(), std::io::Error>(())
	};
	match tokio::time::timeout(METRICS_IO_TIMEOUT, write_fut).await {
		Ok(res) => res,
		Err(_) => Ok(()),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn metrics_register_and_encode_without_panic() {
		// Arrange
		let metrics = Metrics::new();

		// Act
		metrics
			.reconcile_total
			.with_label_values(&["success"])
			.inc();
		metrics
			.requeue_total
			.with_label_values(&["transient"])
			.inc();
		// Use a phase value that the reconciler actually emits via `phase_label`
		// (see reconciler.rs) so this test reflects production label cardinality.
		metrics
			.managed_apps
			.with_label_values(&["Running"])
			.set(3.0);
		let body = metrics.encode();

		// Assert: encoded output contains the metric names.
		let text = String::from_utf8(body).expect("utf8");
		assert!(text.contains("reinhardt_cloud_operator_reconcile_total"));
		assert!(text.contains("reinhardt_cloud_operator_requeue_total"));
		assert!(text.contains("reinhardt_cloud_operator_managed_apps"));
	}

	#[rstest]
	#[case(
		b"GET /metrics HTTP/1.1\r\n",
		MetricsMode::Enabled,
		RequestKind::Metrics
	)]
	#[case(
		b"GET /metrics?debug=1 HTTP/1.1\r\n",
		MetricsMode::Enabled,
		RequestKind::Metrics
	)]
	#[case(
		b"GET /metrics HTTP/1.1\r\n",
		MetricsMode::Disabled,
		RequestKind::Other
	)]
	#[case(
		b"GET /metricsfoo HTTP/1.1\r\n",
		MetricsMode::Enabled,
		RequestKind::Other
	)]
	#[case(
		b"GET /metrics_admin HTTP/1.1\r\n",
		MetricsMode::Enabled,
		RequestKind::Other
	)]
	#[case(
		b"GET /healthz HTTP/1.1\r\n",
		MetricsMode::Enabled,
		RequestKind::Healthz
	)]
	#[case(
		b"POST /metrics HTTP/1.1\r\n",
		MetricsMode::Enabled,
		RequestKind::Other
	)]
	fn classify_metrics_paths(
		#[case] line: &[u8],
		#[case] mode: MetricsMode,
		#[case] expected: RequestKind,
	) {
		// Act
		let actual = classify_request(line, mode);

		// Assert
		assert_eq!(actual, expected);
	}

	#[rstest]
	#[case(b"GET /healthz HTTP/1.1\r\n", true)]
	#[case(b"GET /healthz?ts=1 HTTP/1.1\r\n", true)]
	#[case(b"GET /healthzfoo HTTP/1.1\r\n", false)]
	#[case(b"GET /healthz_admin HTTP/1.1\r\n", false)]
	#[case(b"GET /metrics HTTP/1.1\r\n", false)]
	#[case(b"POST /healthz HTTP/1.1\r\n", false)]
	fn classify_healthz_paths(#[case] line: &[u8], #[case] expected: bool) {
		// Act
		// Use `Disabled` so the `/metrics` request line in this matrix is
		// classified as `Other` rather than `Metrics`, isolating the
		// `/healthz` strictness check from the metrics gate.
		let actual = classify_request(line, MetricsMode::Disabled);

		// Assert: only `/healthz` GETs classify as `Healthz`.
		assert_eq!(actual == RequestKind::Healthz, expected);
	}

	#[rstest]
	#[tokio::test]
	async fn healthz_returns_200_ok_when_metrics_disabled() {
		// Arrange: bind on an ephemeral port with metrics disabled.
		let metrics = Metrics::new();
		let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
			.await
			.expect("bind ephemeral");
		let bind = listener.local_addr().expect("local_addr");
		let handle = tokio::spawn(serve_one(listener, metrics, MetricsMode::Disabled));

		// Act: GET /healthz over a raw TCP client.
		let body = http_get(bind, "/healthz").await;
		handle.abort();

		// Assert
		assert!(
			body.starts_with("HTTP/1.1 200 OK\r\n"),
			"unexpected status line: {body:?}"
		);
		assert!(
			body.contains("Content-Type: text/plain"),
			"missing CT: {body:?}"
		);
		assert!(body.ends_with("ok"), "unexpected body suffix: {body:?}");
	}

	#[rstest]
	#[tokio::test]
	async fn metrics_returns_404_when_disabled() {
		// Arrange
		let metrics = Metrics::new();
		let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
			.await
			.expect("bind ephemeral");
		let bind = listener.local_addr().expect("local_addr");
		let handle = tokio::spawn(serve_one(listener, metrics, MetricsMode::Disabled));

		// Act
		let body = http_get(bind, "/metrics").await;
		handle.abort();

		// Assert
		assert!(
			body.starts_with("HTTP/1.1 404 Not Found\r\n"),
			"unexpected status line: {body:?}"
		);
	}

	/// Helper that accepts exactly one connection from the listener and
	/// invokes `handle_connection`. Used by the integration-style tests
	/// above to keep them self-contained without spawning a long-lived
	/// exporter task.
	async fn serve_one(
		listener: tokio::net::TcpListener,
		metrics: Arc<Metrics>,
		mode: MetricsMode,
	) {
		if let Ok((socket, _)) = listener.accept().await {
			let _ = socket.set_nodelay(true);
			let _ = handle_connection(socket, metrics, mode).await;
		}
	}

	/// Minimal HTTP/1.1 client that sends `GET <path>` and reads the full
	/// response into a `String`.
	async fn http_get(addr: std::net::SocketAddr, path: &str) -> String {
		use tokio::io::{AsyncReadExt, AsyncWriteExt};
		let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
		let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
		stream.write_all(req.as_bytes()).await.expect("write");
		stream.shutdown().await.ok();
		let mut buf = Vec::with_capacity(4096);
		stream.read_to_end(&mut buf).await.expect("read_to_end");
		String::from_utf8_lossy(&buf).into_owned()
	}
}
