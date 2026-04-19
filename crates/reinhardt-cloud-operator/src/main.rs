//! Reinhardt Cloud Kubernetes operator for managing `ReinhardtApp` resources.

mod error;
mod inference;
mod metrics;
mod reconciler;
mod resources;

use std::net::SocketAddr;

use reinhardt_cloud_telemetry::{InMemoryLogService, LogService};


/// Environment variable that selects the log output format.
///
/// Accepted values: `json` (structured JSON, one object per line). Any other
/// value — or unset — selects the default human-readable format.
const LOG_FORMAT_ENV: &str = "REINHARDT_LOG_FORMAT";

/// Log output format selected at startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogFormat {
	Text,
	Json,
}

/// Resolve the log format from an env-var reader.
///
/// Accepts any case-insensitive variant of `"json"`; anything else (including
/// unset) selects [`LogFormat::Text`]. Leading/trailing ASCII whitespace is
/// tolerated.
fn resolve_log_format<F>(env: F) -> LogFormat
where
	F: FnOnce(&str) -> Option<String>,
{
	match env(LOG_FORMAT_ENV) {
		Some(raw) if raw.trim().eq_ignore_ascii_case("json") => LogFormat::Json,
		_ => LogFormat::Text,
	}
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	// Explicitly install rustls CryptoProvider (defense-in-depth, see #314)
	rustls::crypto::ring::default_provider()
		.install_default()
		.ok();

	// Keep the tracing guard alive for the entire program so the OTLP span
	// exporter is flushed on shutdown.
	let _tracing_guard = init_tracing()?;

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
		"Starting reinhardt-cloud operator; enable JSON logs via {}=json",
		LOG_FORMAT_ENV
	);

	let client = kube::Client::try_default().await?;
	reconciler::run(client, operator_metrics).await;

	// Controller loop exited (shutdown signal received or fatal error).
	// Log completion so operators can distinguish clean shutdown from crash.
	tracing::warn!("Controller loop terminated, shutting down");

	Ok(())
}

/// Initialize OpenTelemetry tracing and the global `tracing` subscriber.
///
/// Delegates to [`reinhardt_cloud_telemetry::init_tracing`], which honors
/// standard OTel env vars (`OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_SERVICE_NAME`,
/// `OTEL_TRACES_SAMPLER`, `OTEL_TRACES_SAMPLER_ARG`). When
/// `OTEL_EXPORTER_OTLP_ENDPOINT` is unset, no OTLP exporter is installed and
/// no OpenTelemetry layer is attached.
///
/// The JSON log format is selected by `REINHARDT_LOG_FORMAT=json` (see
/// [`LOG_FORMAT_ENV`]); otherwise a human-readable formatter is used.
fn init_tracing() -> anyhow::Result<reinhardt_cloud_telemetry::TracingGuard> {
	let json_logs = matches!(
		resolve_log_format(|key| std::env::var(key).ok()),
		LogFormat::Json
	);
	let config =
		reinhardt_cloud_telemetry::TracingConfig::from_env("reinhardt-cloud-operator", json_logs);
	reinhardt_cloud_telemetry::init_tracing(config).map_err(anyhow::Error::from)
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	fn env_with<'a>(key: &'a str, value: Option<&'a str>) -> impl Fn(&str) -> Option<String> + 'a {
		move |requested| {
			if requested == key {
				value.map(str::to_owned)
			} else {
				None
			}
		}
	}

	#[rstest]
	fn resolve_json_when_env_is_json_literal() {
		// Arrange
		let env = env_with(LOG_FORMAT_ENV, Some("json"));

		// Act
		let format = resolve_log_format(env);

		// Assert
		assert_eq!(format, LogFormat::Json);
	}

	#[rstest]
	#[case("JSON")]
	#[case("Json")]
	#[case("  json  ")]
	#[case("json\n")]
	fn resolve_json_tolerates_case_and_whitespace(#[case] raw: &str) {
		// Arrange
		let env = env_with(LOG_FORMAT_ENV, Some(raw));

		// Act
		let format = resolve_log_format(env);

		// Assert
		assert_eq!(format, LogFormat::Json, "raw = {raw:?}");
	}

	#[rstest]
	#[case("text")]
	#[case("plain")]
	#[case("")]
	#[case("jsonl")]
	fn resolve_text_when_env_is_not_json(#[case] raw: &str) {
		// Arrange
		let env = env_with(LOG_FORMAT_ENV, Some(raw));

		// Act
		let format = resolve_log_format(env);

		// Assert
		assert_eq!(format, LogFormat::Text, "raw = {raw:?}");
	}

	#[rstest]
	fn resolve_text_when_env_is_unset() {
		// Arrange
		let env = env_with(LOG_FORMAT_ENV, None);

		// Act
		let format = resolve_log_format(env);

		// Assert
		assert_eq!(format, LogFormat::Text);
	}
}
