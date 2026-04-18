//! Reinhardt Cloud Kubernetes operator for managing `ReinhardtApp` resources.

mod error;
mod inference;
mod reconciler;
mod resources;

use reinhardt_cloud_telemetry::{InMemoryLogService, LogService};
use tracing_subscriber::{EnvFilter, fmt};

/// Environment variable that selects the log output format.
///
/// Accepted values: `json` (structured JSON, one object per line). Any other
/// value — or unset — selects the default human-readable format.
const LOG_FORMAT_ENV: &str = "REINHARDT_LOG_FORMAT";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	// Explicitly install rustls CryptoProvider (defense-in-depth, see #314)
	rustls::crypto::ring::default_provider()
		.install_default()
		.ok();

	init_tracing()?;

	tracing::info!("Starting reinhardt-cloud operator");

	// Surface the default telemetry retention policy at startup so operators
	// can confirm the log-schema crate is wired in ahead of Phase 4 plumbing.
	let default_policy = InMemoryLogService::default().retention_policy();
	tracing::debug!(
		capacity = default_policy.capacity,
		ttl_secs = default_policy.ttl.as_secs(),
		"Default in-memory log retention policy"
	);
	tracing::info!(
		log_schema = "reinhardt-cloud-telemetry/v1",
		"Structured log schema available; enable JSON format via {}=json",
		LOG_FORMAT_ENV
	);

	let client = kube::Client::try_default().await?;
	reconciler::run(client).await;

	// Controller loop exited (shutdown signal received or fatal error).
	// Log completion so operators can distinguish clean shutdown from crash.
	tracing::warn!("Controller loop terminated, shutting down");

	Ok(())
}

/// Initialize the global `tracing` subscriber.
///
/// Selects the JSON formatter when `REINHARDT_LOG_FORMAT=json` is set
/// (case-insensitive); otherwise falls back to the default human-readable
/// formatter. The `RUST_LOG` env var still drives level filtering in both
/// modes.
fn init_tracing() -> anyhow::Result<()> {
	let env_filter =
		EnvFilter::from_default_env().add_directive("reinhardt_cloud_operator=info".parse()?);

	let json_mode = std::env::var(LOG_FORMAT_ENV)
		.map(|v| v.eq_ignore_ascii_case("json"))
		.unwrap_or(false);

	if json_mode {
		fmt()
			.json()
			.flatten_event(true)
			.with_current_span(true)
			.with_span_list(false)
			.with_env_filter(env_filter)
			.init();
	} else {
		fmt().with_env_filter(env_filter).init();
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use rstest::rstest;

	#[rstest]
	fn json_mode_detection_respects_env_value() {
		// Arrange
		let value = "json";

		// Act
		let is_json = value.eq_ignore_ascii_case("json");

		// Assert
		assert!(is_json);
	}

	#[rstest]
	#[case("text")]
	#[case("plain")]
	#[case("")]
	#[case("JSONL")]
	#[case("jsonp")]
	fn json_mode_detection_rejects_other_values(#[case] value: &str) {
		// Act
		let is_json = value.eq_ignore_ascii_case("json");

		// Assert
		assert!(!is_json, "value {value:?} should not select JSON mode");
	}
}
