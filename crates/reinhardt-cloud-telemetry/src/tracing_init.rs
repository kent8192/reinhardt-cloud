//! OpenTelemetry-backed tracing initialization for reinhardt-cloud.
//!
//! Zero overhead when `OTEL_EXPORTER_OTLP_ENDPOINT` is unset: the OTel layer
//! is omitted entirely and the subscriber stack contains only the env filter
//! and the JSON/text fmt layer.

mod layer;
mod manual;

pub use layer::TraceContextLogLayer;
pub use manual::{TraceContext, current_trace_context};

use std::env;
use std::time::Duration;

use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler, SdkTracerProvider};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

const OTLP_ENDPOINT_ENV: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";
const SERVICE_NAME_ENV: &str = "OTEL_SERVICE_NAME";
const SAMPLER_ENV: &str = "OTEL_TRACES_SAMPLER_ARG";
const DEFAULT_SAMPLE_RATIO: f64 = 0.1;
const EXPORTER_TIMEOUT_SECS: u64 = 5;
const TRACER_NAME: &str = "reinhardt-cloud";

/// Runtime-configurable tracing setup.
#[derive(Debug, Clone)]
pub struct TracingConfig {
	/// Logical service name reported in the OTel `service.name` resource attribute.
	pub service_name: String,
	/// OTLP gRPC endpoint (e.g. `http://otel-collector:4317`). `None` disables OTel export.
	pub otlp_endpoint: Option<String>,
	/// Head-sampling ratio in the range `[0.0, 1.0]`.
	pub sample_ratio: f64,
	/// Emit JSON-formatted logs on stdout when `true`; text-formatted otherwise.
	pub json_logs: bool,
}

impl TracingConfig {
	/// Build a `TracingConfig` from standard OTel environment variables.
	pub fn from_env(default_service: &str, json_logs: bool) -> Self {
		Self {
			service_name: env::var(SERVICE_NAME_ENV)
				.ok()
				.filter(|s| !s.is_empty())
				.unwrap_or_else(|| default_service.to_string()),
			otlp_endpoint: env::var(OTLP_ENDPOINT_ENV).ok().filter(|s| !s.is_empty()),
			sample_ratio: env::var(SAMPLER_ENV)
				.ok()
				.and_then(|raw| parse_ratio(&raw))
				.unwrap_or(DEFAULT_SAMPLE_RATIO),
			json_logs,
		}
	}
}

fn parse_ratio(raw: &str) -> Option<f64> {
	raw.split_once('=')
		.map(|(_, v)| v)
		.unwrap_or(raw)
		.parse::<f64>()
		.ok()
		.filter(|v| (0.0..=1.0).contains(v))
}

/// Guard that flushes pending spans on drop.
pub struct TracingGuard {
	inner: TracingGuardInner,
}

enum TracingGuardInner {
	Noop,
	WithProvider(SdkTracerProvider),
}

impl std::fmt::Debug for TracingGuard {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match &self.inner {
			TracingGuardInner::Noop => f.debug_struct("TracingGuard").field("otel", &false).finish(),
			TracingGuardInner::WithProvider(_) => {
				f.debug_struct("TracingGuard").field("otel", &true).finish()
			}
		}
	}
}

impl Drop for TracingGuard {
	fn drop(&mut self) {
		if let TracingGuardInner::WithProvider(provider) = &self.inner {
			// Shutdown must not panic on drop; swallow errors.
			let _ = provider.shutdown();
		}
	}
}

/// Initialize the global tracing subscriber.
///
/// Returns a [`TracingGuard`] that flushes pending spans when dropped. When
/// `config.otlp_endpoint` is `None`, no OpenTelemetry layer is attached and
/// the subscriber stack contains only the env filter and an fmt layer.
pub fn init_tracing(config: TracingConfig) -> anyhow::Result<TracingGuard> {
	let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

	let fmt_layer = if config.json_logs {
		tracing_subscriber::fmt::layer()
			.json()
			.with_current_span(true)
			.with_span_list(false)
			.boxed()
	} else {
		tracing_subscriber::fmt::layer().boxed()
	};

	let registry = tracing_subscriber::registry()
		.with(filter)
		.with(TraceContextLogLayer::new())
		.with(fmt_layer);

	match &config.otlp_endpoint {
		Some(endpoint) => {
			let exporter = SpanExporter::builder()
				.with_tonic()
				.with_endpoint(endpoint)
				.with_timeout(Duration::from_secs(EXPORTER_TIMEOUT_SECS))
				.build()
				.map_err(|e| anyhow::anyhow!("failed to build OTLP span exporter: {e}"))?;

			let resource = Resource::builder_empty()
				.with_attributes([KeyValue::new("service.name", config.service_name.clone())])
				.build();

			let sampler = Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
				config.sample_ratio,
			)));

			let provider = SdkTracerProvider::builder()
				.with_batch_exporter(exporter)
				.with_sampler(sampler)
				.with_id_generator(RandomIdGenerator::default())
				.with_resource(resource)
				.build();

			opentelemetry::global::set_tracer_provider(provider.clone());

			let tracer = provider.tracer(TRACER_NAME);
			let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

			registry
				.with(otel_layer)
				.try_init()
				.map_err(|e| anyhow::anyhow!("failed to install tracing subscriber: {e}"))?;

			Ok(TracingGuard {
				inner: TracingGuardInner::WithProvider(provider),
			})
		}
		None => {
			registry
				.try_init()
				.map_err(|e| anyhow::anyhow!("failed to install tracing subscriber: {e}"))?;
			Ok(TracingGuard {
				inner: TracingGuardInner::Noop,
			})
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn parse_ratio_accepts_bare_float() {
		assert_eq!(parse_ratio("0.5"), Some(0.5));
	}

	#[rstest]
	fn parse_ratio_accepts_sampler_expression() {
		assert_eq!(parse_ratio("parentbased_traceidratio=0.25"), Some(0.25));
	}

	#[rstest]
	fn parse_ratio_rejects_out_of_range() {
		assert_eq!(parse_ratio("2.0"), None);
		assert_eq!(parse_ratio("-0.1"), None);
	}

	#[rstest]
	fn parse_ratio_rejects_garbage() {
		assert_eq!(parse_ratio("not-a-number"), None);
	}

	#[rstest]
	fn tracing_config_from_env_uses_default_service() {
		// Arrange / Act
		let cfg = TracingConfig::from_env("test-service", false);

		// Assert: service_name is populated and json_logs propagates.
		assert!(!cfg.service_name.is_empty());
		assert!(!cfg.json_logs);
		assert!((0.0..=1.0).contains(&cfg.sample_ratio));
	}
}
