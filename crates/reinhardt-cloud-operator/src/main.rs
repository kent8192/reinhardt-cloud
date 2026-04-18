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

	let format = resolve_log_format(|key| std::env::var(key).ok());
	match format {
		LogFormat::Json => {
			fmt()
				.json()
				.flatten_event(true)
				.with_current_span(true)
				.with_span_list(false)
				.with_env_filter(env_filter)
				.init();
		}
		LogFormat::Text => {
			fmt().with_env_filter(env_filter).init();
		}
	}

	Ok(())
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
