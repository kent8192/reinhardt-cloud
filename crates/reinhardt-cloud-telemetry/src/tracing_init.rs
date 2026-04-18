//! OpenTelemetry-backed tracing initialization for reinhardt-cloud.
//!
//! Zero overhead when `OTEL_EXPORTER_OTLP_ENDPOINT` is unset: the OTel layer
//! is omitted entirely and the subscriber stack contains only the env filter
//! and the JSON/text fmt layer.

mod layer;
mod manual;

pub use layer::{TraceContextExtension, TraceContextLogLayer};
pub use manual::{TraceContext, current_trace_context};

use std::env;
use std::time::Duration;

use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{ExporterBuildError, SpanExporter, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler, SdkTracerProvider};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::util::TryInitError;

const OTLP_ENDPOINT_ENV: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";
const SERVICE_NAME_ENV: &str = "OTEL_SERVICE_NAME";
const SAMPLER_ENV: &str = "OTEL_TRACES_SAMPLER";
const SAMPLER_ARG_ENV: &str = "OTEL_TRACES_SAMPLER_ARG";
/// Default head-sampling ratio applied when `OTEL_TRACES_SAMPLER_ARG` is unset
/// or the selected sampler does not require an argument.
pub const DEFAULT_SAMPLE_RATIO: f64 = 0.1;
const EXPORTER_TIMEOUT_SECS: u64 = 5;
const TRACER_NAME: &str = "reinhardt-cloud";

/// Errors returned by [`init_tracing`].
#[derive(Debug, thiserror::Error)]
pub enum TracingInitError {
	/// The OTLP span exporter failed to build (invalid endpoint, transport setup, ...).
	#[error("failed to build OTLP span exporter: {0}")]
	ExporterBuild(#[source] ExporterBuildError),
	/// Installing the tracing subscriber failed (typically: a global subscriber is already set).
	#[error("failed to install tracing subscriber: {0}")]
	SubscriberInstall(#[source] TryInitError),
}

/// Sampler strategy selected via `OTEL_TRACES_SAMPLER`.
///
/// Matches the variants defined by the OpenTelemetry specification. Values not
/// listed here fall back to [`SamplerKind::ParentBasedTraceIdRatio`] (the default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SamplerKind {
	/// Sample every span.
	AlwaysOn,
	/// Drop every span.
	AlwaysOff,
	/// Sample based purely on a trace-ID ratio (no parent consultation).
	TraceIdRatio,
	/// Honor parent sampling decision; fall back to trace-ID ratio for roots.
	#[default]
	ParentBasedTraceIdRatio,
	/// Honor parent sampling decision; always sample roots.
	ParentBasedAlwaysOn,
	/// Honor parent sampling decision; never sample roots.
	ParentBasedAlwaysOff,
}

impl SamplerKind {
	fn from_env_value(raw: &str) -> Option<Self> {
		match raw.trim().to_ascii_lowercase().as_str() {
			"always_on" => Some(Self::AlwaysOn),
			"always_off" => Some(Self::AlwaysOff),
			"traceidratio" => Some(Self::TraceIdRatio),
			"parentbased_traceidratio" => Some(Self::ParentBasedTraceIdRatio),
			"parentbased_always_on" => Some(Self::ParentBasedAlwaysOn),
			"parentbased_always_off" => Some(Self::ParentBasedAlwaysOff),
			_ => None,
		}
	}
}

/// Runtime-configurable tracing setup.
#[derive(Debug, Clone)]
pub struct TracingConfig {
	/// Logical service name reported in the OTel `service.name` resource attribute.
	pub service_name: String,
	/// OTLP gRPC endpoint (e.g. `http://otel-collector:4317`). `None` disables OTel export.
	pub otlp_endpoint: Option<String>,
	/// Sampler strategy, parsed from `OTEL_TRACES_SAMPLER`.
	pub sampler_kind: SamplerKind,
	/// Head-sampling ratio in the range `[0.0, 1.0]`, parsed from `OTEL_TRACES_SAMPLER_ARG`.
	///
	/// Only consulted for ratio-based samplers.
	pub sample_ratio: f64,
	/// Emit JSON-formatted logs on stdout when `true`; text-formatted otherwise.
	pub json_logs: bool,
}

impl TracingConfig {
	/// Build a `TracingConfig` from standard OTel environment variables.
	///
	/// Honors `OTEL_SERVICE_NAME`, `OTEL_EXPORTER_OTLP_ENDPOINT`,
	/// `OTEL_TRACES_SAMPLER`, and `OTEL_TRACES_SAMPLER_ARG`.
	pub fn from_env(default_service: &str, json_logs: bool) -> Self {
		Self {
			service_name: env::var(SERVICE_NAME_ENV)
				.ok()
				.filter(|s| !s.is_empty())
				.unwrap_or_else(|| default_service.to_string()),
			otlp_endpoint: env::var(OTLP_ENDPOINT_ENV).ok().filter(|s| !s.is_empty()),
			sampler_kind: env::var(SAMPLER_ENV)
				.ok()
				.and_then(|raw| SamplerKind::from_env_value(&raw))
				.unwrap_or_default(),
			sample_ratio: env::var(SAMPLER_ARG_ENV)
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

fn build_sampler(kind: SamplerKind, ratio: f64) -> Sampler {
	match kind {
		SamplerKind::AlwaysOn => Sampler::AlwaysOn,
		SamplerKind::AlwaysOff => Sampler::AlwaysOff,
		SamplerKind::TraceIdRatio => Sampler::TraceIdRatioBased(ratio),
		SamplerKind::ParentBasedTraceIdRatio => {
			Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(ratio)))
		}
		SamplerKind::ParentBasedAlwaysOn => Sampler::ParentBased(Box::new(Sampler::AlwaysOn)),
		SamplerKind::ParentBasedAlwaysOff => Sampler::ParentBased(Box::new(Sampler::AlwaysOff)),
	}
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
			TracingGuardInner::Noop => f
				.debug_struct("TracingGuard")
				.field("otel", &false)
				.finish(),
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
///
/// # Errors
///
/// - [`TracingInitError::ExporterBuild`] if constructing the OTLP exporter fails.
/// - [`TracingInitError::SubscriberInstall`] if installing the global subscriber fails.
pub fn init_tracing(config: TracingConfig) -> Result<TracingGuard, TracingInitError> {
	// Install the W3C TraceContext propagator globally so that traceparent
	// headers can be extracted and injected across process boundaries.
	opentelemetry::global::set_text_map_propagator(
		opentelemetry_sdk::propagation::TraceContextPropagator::new(),
	);

	let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

	// Build the fmt layer unboxed; boxing happens against the final
	// subscriber type in each branch below.
	let json_logs = config.json_logs;

	match &config.otlp_endpoint {
		Some(endpoint) => {
			let exporter = SpanExporter::builder()
				.with_tonic()
				.with_endpoint(endpoint)
				.with_timeout(Duration::from_secs(EXPORTER_TIMEOUT_SECS))
				.build()
				.map_err(TracingInitError::ExporterBuild)?;

			let resource = Resource::builder_empty()
				.with_attributes([KeyValue::new("service.name", config.service_name.clone())])
				.build();

			let sampler = build_sampler(config.sampler_kind, config.sample_ratio);

			let provider = SdkTracerProvider::builder()
				.with_batch_exporter(exporter)
				.with_sampler(sampler)
				.with_id_generator(RandomIdGenerator::default())
				.with_resource(resource)
				.build();

			opentelemetry::global::set_tracer_provider(provider.clone());

			let tracer = provider.tracer(TRACER_NAME);
			let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

			// Layer ordering: env filter first, then the OTel layer (so it
			// attaches OtelData before TraceContextLogLayer reads it), then
			// our enrichment layer, then the fmt layer that consumes both.
			let base = tracing_subscriber::registry()
				.with(filter)
				.with(otel_layer)
				.with(TraceContextLogLayer::new());

			if json_logs {
				base.with(
					tracing_subscriber::fmt::layer()
						.json()
						.with_current_span(true)
						.with_span_list(false),
				)
				.try_init()
				.map_err(TracingInitError::SubscriberInstall)?;
			} else {
				base.with(tracing_subscriber::fmt::layer())
					.try_init()
					.map_err(TracingInitError::SubscriberInstall)?;
			}

			Ok(TracingGuard {
				inner: TracingGuardInner::WithProvider(provider),
			})
		}
		None => {
			let base = tracing_subscriber::registry()
				.with(filter)
				.with(TraceContextLogLayer::new());

			if json_logs {
				base.with(
					tracing_subscriber::fmt::layer()
						.json()
						.with_current_span(true)
						.with_span_list(false),
				)
				.try_init()
				.map_err(TracingInitError::SubscriberInstall)?;
			} else {
				base.with(tracing_subscriber::fmt::layer())
					.try_init()
					.map_err(TracingInitError::SubscriberInstall)?;
			}
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
	use serial_test::serial;

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
	#[case("always_on", SamplerKind::AlwaysOn)]
	#[case("always_off", SamplerKind::AlwaysOff)]
	#[case("traceidratio", SamplerKind::TraceIdRatio)]
	#[case("parentbased_traceidratio", SamplerKind::ParentBasedTraceIdRatio)]
	#[case("parentbased_always_on", SamplerKind::ParentBasedAlwaysOn)]
	#[case("parentbased_always_off", SamplerKind::ParentBasedAlwaysOff)]
	#[case("PARENTBASED_TRACEIDRATIO", SamplerKind::ParentBasedTraceIdRatio)]
	fn sampler_kind_parses_known_values(#[case] raw: &str, #[case] expected: SamplerKind) {
		assert_eq!(SamplerKind::from_env_value(raw), Some(expected));
	}

	#[rstest]
	fn sampler_kind_rejects_unknown() {
		assert_eq!(SamplerKind::from_env_value("nope"), None);
	}

	#[rstest]
	#[serial(env)]
	fn tracing_config_from_env_uses_defaults_when_unset() {
		// Arrange: strip any OTEL env vars the host shell may carry.
		// SAFETY: Test is serialized on the `env` group; no other thread
		// reads/writes these variables concurrently.
		unsafe {
			std::env::remove_var(SERVICE_NAME_ENV);
			std::env::remove_var(OTLP_ENDPOINT_ENV);
			std::env::remove_var(SAMPLER_ENV);
			std::env::remove_var(SAMPLER_ARG_ENV);
		}

		// Act
		let cfg = TracingConfig::from_env("test-service", false);

		// Assert
		assert_eq!(cfg.service_name, "test-service");
		assert_eq!(cfg.otlp_endpoint, None);
		assert_eq!(cfg.sampler_kind, SamplerKind::ParentBasedTraceIdRatio);
		assert_eq!(cfg.sample_ratio, DEFAULT_SAMPLE_RATIO);
		assert!(!cfg.json_logs);
	}

	#[rstest]
	#[serial(env)]
	fn tracing_config_from_env_reads_sampler_vars() {
		// Arrange
		// SAFETY: Test is serialized on the `env` group.
		unsafe {
			std::env::set_var(SAMPLER_ENV, "traceidratio");
			std::env::set_var(SAMPLER_ARG_ENV, "0.25");
			std::env::remove_var(SERVICE_NAME_ENV);
			std::env::remove_var(OTLP_ENDPOINT_ENV);
		}

		// Act
		let cfg = TracingConfig::from_env("svc", false);

		// Assert
		assert_eq!(cfg.sampler_kind, SamplerKind::TraceIdRatio);
		assert_eq!(cfg.sample_ratio, 0.25);

		// Cleanup.
		// SAFETY: Test is serialized on the `env` group.
		unsafe {
			std::env::remove_var(SAMPLER_ENV);
			std::env::remove_var(SAMPLER_ARG_ENV);
		}
	}
}
