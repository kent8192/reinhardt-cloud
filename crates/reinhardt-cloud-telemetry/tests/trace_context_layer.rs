//! Integration test: verify [`TraceContextLogLayer`] attaches
//! [`TraceContextExtension`] to span extensions when layered alongside
//! `tracing_opentelemetry::OpenTelemetryLayer`.
//!
//! Lives in its own test binary so the global tracer provider installed here
//! cannot interfere with other unit tests in the crate.

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::SdkTracerProvider;
use reinhardt_cloud_telemetry::{TraceContextExtension, TraceContextLogLayer};
use rstest::rstest;
use std::sync::{Arc, Mutex};
use tracing::Id;
use tracing::subscriber::with_default;
use tracing_subscriber::Registry;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;

/// A capture layer that reads [`TraceContextExtension`] from span extensions
/// inside `on_close`, after all `on_enter` callbacks have run.
struct CaptureLayer {
	captured: Arc<Mutex<Option<TraceContextExtension>>>,
}

impl<S> tracing_subscriber::Layer<S> for CaptureLayer
where
	S: tracing::Subscriber + for<'lookup> LookupSpan<'lookup>,
{
	fn on_close(&self, id: Id, ctx: Context<'_, S>) {
		if let Some(span_ref) = ctx.span(&id) {
			let extensions = span_ref.extensions();
			if let Some(ext) = extensions.get::<TraceContextExtension>() {
				*self.captured.lock().unwrap() = Some(ext.clone());
			}
		}
	}
}

/// Verifies that `TraceContextLogLayer` inserts a [`TraceContextExtension`]
/// with valid, non-zero trace and span IDs into the span's extensions.
#[rstest]
fn layer_attaches_trace_context_extension_to_span() {
	// Arrange: in-process SDK tracer provider with no exporter — still
	// generates valid span IDs via the default random id generator.
	let provider = SdkTracerProvider::builder().build();
	let tracer = provider.tracer("reinhardt-cloud-telemetry-test");
	let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

	let captured: Arc<Mutex<Option<TraceContextExtension>>> = Arc::new(Mutex::new(None));
	let capture_layer = CaptureLayer {
		captured: Arc::clone(&captured),
	};

	// Layer ordering: OTel layer first (populates OtelData), then
	// TraceContextLogLayer (reads OtelData and inserts TraceContextExtension),
	// then our capture layer (reads TraceContextExtension on close).
	let subscriber = Registry::default()
		.with(otel_layer)
		.with(TraceContextLogLayer::new())
		.with(capture_layer);

	// Act: enter and exit a span so on_close fires.
	with_default(subscriber, || {
		let span = tracing::info_span!("integration.test.root");
		// Explicitly drop the guard so on_close fires before with_default returns.
		drop(span.enter());
		// Drop the span handle to trigger on_close.
	});

	// Assert: CaptureLayer saw a TraceContextExtension with valid hex IDs.
	let ext = captured.lock().unwrap().take().expect(
		"TraceContextExtension must be attached to span extensions by TraceContextLogLayer",
	);

	assert_eq!(
		ext.trace_id.len(),
		32,
		"trace_id must be 32-char hex (got {:?})",
		ext.trace_id
	);
	assert_eq!(
		ext.span_id.len(),
		16,
		"span_id must be 16-char hex (got {:?})",
		ext.span_id
	);
	assert_ne!(
		ext.trace_id,
		"0".repeat(32),
		"trace_id must not be all zeros"
	);
	assert_ne!(ext.span_id, "0".repeat(16), "span_id must not be all zeros");
}
