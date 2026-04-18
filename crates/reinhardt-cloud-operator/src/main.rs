//! Reinhardt Cloud Kubernetes operator for managing `ReinhardtApp` resources.

mod error;
mod inference;
mod metrics;
mod reconciler;
mod resources;

use std::net::SocketAddr;

use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	// Explicitly install rustls CryptoProvider (defense-in-depth, see #314)
	rustls::crypto::ring::default_provider()
		.install_default()
		.ok();

	fmt()
		.with_env_filter(
			EnvFilter::from_default_env().add_directive("reinhardt_cloud_operator=info".parse()?),
		)
		.init();

	tracing::info!("Starting reinhardt-cloud operator");

	let operator_metrics = metrics::Metrics::new();

	// Launch the Prometheus exporter on :9090 unless overridden via env.
	// Errors binding the exporter are logged by the spawned task; the
	// operator keeps running so that reconciliation is not blocked by a
	// metrics port conflict.
	let bind: SocketAddr = std::env::var("REINHARDT_CLOUD_METRICS_ADDR")
		.ok()
		.and_then(|v| v.parse().ok())
		.unwrap_or_else(|| "0.0.0.0:9090".parse().expect("static bind address"));
	metrics::spawn_exporter(operator_metrics.clone(), bind);

	let client = kube::Client::try_default().await?;
	reconciler::run(client, operator_metrics).await;

	// Controller loop exited (shutdown signal received or fatal error).
	// Log completion so operators can distinguish clean shutdown from crash.
	tracing::warn!("Controller loop terminated, shutting down");

	Ok(())
}
