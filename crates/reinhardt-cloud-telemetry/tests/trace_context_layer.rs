//! Integration test: verify [`TraceContextLogLayer`] populates its extension
//! when layered alongside `tracing_opentelemetry::OpenTelemetryLayer`.
//!
//! Lives in its own test binary so the global tracer provider installed here
//! cannot interfere with other unit tests in the crate.

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::SdkTracerProvider;
use reinhardt_cloud_telemetry::{TraceContext, TraceContextLogLayer, current_trace_context};
use tracing_subscriber::Registry;
use tracing_subscriber::layer::SubscriberExt;

#[test]
fn trace_context_is_populated_when_otel_layer_installed() {
	// Arrange: in-process SDK tracer provider with no exporter — still
	// generates valid span IDs via the default random id generator.
	let provider = SdkTracerProvider::builder().build();
	let tracer = provider.tracer("reinhardt-cloud-telemetry-test");
	let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

	let subscriber = Registry::default()
		.with(otel_layer) // must come first so OtelData is attached
		.with(TraceContextLogLayer::new());

	let ctx_inside: Option<TraceContext> = tracing::subscriber::with_default(subscriber, || {
		let span = tracing::info_span!("integration.test.root");
		let _enter = span.enter();
		current_trace_context()
	});

	// Assert: helper returned a valid context with non-empty hex IDs.
	let ctx = ctx_inside.expect("expected trace context inside instrumented span");
	assert_eq!(
		ctx.trace_id.len(),
		32,
		"trace_id must be 32-char hex (got {:?})",
		ctx.trace_id
	);
	assert_eq!(
		ctx.span_id.len(),
		16,
		"span_id must be 16-char hex (got {:?})",
		ctx.span_id
	);
	assert_ne!(ctx.trace_id, "0".repeat(32));
	assert_ne!(ctx.span_id, "0".repeat(16));
}
