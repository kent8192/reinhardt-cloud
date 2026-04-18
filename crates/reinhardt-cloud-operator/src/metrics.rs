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
//!   issued by the error policy, labeled by backoff class.
//! - `reinhardt_cloud_operator_managed_apps{phase}` — gauge of `ReinhardtApp`
//!   objects currently tracked by the reconciler, labeled by phase.

use std::sync::Arc;

use prometheus::{
	CounterVec, Encoder, GaugeVec, HistogramVec, Opts, Registry, TextEncoder, histogram_opts,
};

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

/// Spawn a minimal HTTP `/metrics` server on the given bind address.
///
/// The server is implemented with a raw `tokio::net::TcpListener` to
/// avoid pulling hyper/axum into the operator binary just for a single
/// endpoint. It serves `GET /metrics` with the Prometheus text format
/// and replies `404` to any other request.
pub(crate) fn spawn_exporter(
	metrics: Arc<Metrics>,
	bind: std::net::SocketAddr,
) -> tokio::task::JoinHandle<()> {
	tokio::spawn(async move {
		let listener = match tokio::net::TcpListener::bind(bind).await {
			Ok(l) => l,
			Err(err) => {
				tracing::error!("Failed to bind metrics exporter on {bind}: {err}");
				return;
			}
		};
		tracing::info!("Metrics exporter listening on http://{bind}/metrics");

		loop {
			let (socket, _) = match listener.accept().await {
				Ok(s) => s,
				Err(err) => {
					tracing::warn!("metrics listener accept error: {err}");
					continue;
				}
			};
			let metrics = Arc::clone(&metrics);
			tokio::spawn(async move {
				if let Err(err) = handle_connection(socket, metrics).await {
					tracing::debug!("metrics connection closed with error: {err}");
				}
			});
		}
	})
}

async fn handle_connection(
	mut socket: tokio::net::TcpStream,
	metrics: Arc<Metrics>,
) -> std::io::Result<()> {
	use tokio::io::{AsyncReadExt, AsyncWriteExt};

	// Read a small request header (we only need the request line).
	let mut buf = [0u8; 1024];
	let n = socket.read(&mut buf).await?;
	if n == 0 {
		return Ok(());
	}
	let head = &buf[..n];
	let is_metrics = head.starts_with(b"GET /metrics");

	if is_metrics {
		let body = metrics.encode();
		let header = format!(
			"HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
			body.len()
		);
		socket.write_all(header.as_bytes()).await?;
		socket.write_all(&body).await?;
	} else {
		let msg = b"not found";
		let header = format!(
			"HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
			msg.len()
		);
		socket.write_all(header.as_bytes()).await?;
		socket.write_all(msg).await?;
	}
	socket.shutdown().await?;
	Ok(())
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
		metrics.managed_apps.with_label_values(&["Ready"]).set(3.0);
		let body = metrics.encode();

		// Assert: encoded output contains the metric names.
		let text = String::from_utf8(body).expect("utf8");
		assert!(text.contains("reinhardt_cloud_operator_reconcile_total"));
		assert!(text.contains("reinhardt_cloud_operator_requeue_total"));
		assert!(text.contains("reinhardt_cloud_operator_managed_apps"));
	}
}
