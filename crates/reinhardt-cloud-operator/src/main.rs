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

	// Launch the Prometheus exporter only when explicitly enabled. This
	// matches the Helm chart's `metrics.enabled` flag: when disabled the
	// operator does not open a listening socket, avoiding unexpected open
	// ports and port conflicts. Either setting `REINHARDT_CLOUD_METRICS_ENABLED=true`
	// or providing `REINHARDT_CLOUD_METRICS_ADDR` turns the exporter on.
	// Errors binding the exporter are logged by the spawned task; the
	// operator keeps running so that reconciliation is not blocked by a
	// metrics port conflict.
	let metrics_addr = std::env::var("REINHARDT_CLOUD_METRICS_ADDR").ok();
	let metrics_enabled = std::env::var("REINHARDT_CLOUD_METRICS_ENABLED")
		.ok()
		.is_some_and(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "True"));
	if metrics_enabled || metrics_addr.is_some() {
		// When `REINHARDT_CLOUD_METRICS_ADDR` is present but unparsable, do
		// NOT silently fall back to `0.0.0.0:9090` — that could expose the
		// exporter on all interfaces or collide with another listener while
		// hiding a configuration mistake. Refuse to start the exporter and
		// surface the error so the operator (or its operator) can fix the
		// supplied value.
		let bind: Option<SocketAddr> = match metrics_addr.as_deref() {
			Some(raw) => match raw.parse::<SocketAddr>() {
				Ok(addr) => Some(addr),
				Err(err) => {
					tracing::error!(
						"Invalid REINHARDT_CLOUD_METRICS_ADDR={raw:?}: {err}; metrics exporter disabled"
					);
					None
				}
			},
			None => Some("0.0.0.0:9090".parse().expect("static bind address")),
		};
		if let Some(bind) = bind {
			metrics::spawn_exporter(operator_metrics.clone(), bind);
		}
	} else {
		tracing::info!(
			"Prometheus metrics exporter disabled (set REINHARDT_CLOUD_METRICS_ENABLED=true or REINHARDT_CLOUD_METRICS_ADDR to enable)"
		);
	}

	let client = kube::Client::try_default().await?;
	reconciler::run(client, operator_metrics).await;

	// Controller loop exited (shutdown signal received or fatal error).
	// Log completion so operators can distinguish clean shutdown from crash.
	tracing::warn!("Controller loop terminated, shutting down");

	Ok(())
}
