//! Tracing subscriber layer bridging OpenTelemetry span context to log records.
//!
//! On span creation, this layer reads the active OpenTelemetry context via
//! [`tracing_opentelemetry::OpenTelemetrySpanExt::context`] and stores the
//! resulting 32-hex `trace_id` and 16-hex `span_id` as a span extension so
//! downstream fmt layers / log enrichers can correlate logs with traces.

use opentelemetry::trace::TraceContextExt;
use tracing::span::Attributes;
use tracing::{Id, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

/// Trace context captured at span creation time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceContextExtension {
	/// 32-hex-char OpenTelemetry trace ID.
	pub trace_id: String,
	/// 16-hex-char OpenTelemetry span ID.
	pub span_id: String,
}

/// Layer that enriches tracing spans with OpenTelemetry trace/span IDs so they
/// can be embedded in structured log output.
#[derive(Debug, Default, Clone, Copy)]
pub struct TraceContextLogLayer;

impl TraceContextLogLayer {
	/// Create a new `TraceContextLogLayer`.
	pub fn new() -> Self {
		Self
	}
}

impl<S> Layer<S> for TraceContextLogLayer
where
	S: tracing::Subscriber + for<'lookup> LookupSpan<'lookup>,
{
	fn on_new_span(&self, _attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
		let Some(span_ref) = ctx.span(id) else {
			return;
		};

		// Read the OTel context bound to this tracing span by the
		// `tracing-opentelemetry` layer (if installed).
		let tracing_span = Span::current();
		let otel_cx = tracing_span.context();
		let otel_span = otel_cx.span();
		let span_ctx = otel_span.span_context();

		if !span_ctx.is_valid() {
			return;
		}

		let extension = TraceContextExtension {
			trace_id: span_ctx.trace_id().to_string(),
			span_id: span_ctx.span_id().to_string(),
		};

		let mut extensions = span_ref.extensions_mut();
		if extensions.get_mut::<TraceContextExtension>().is_none() {
			extensions.insert(extension);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;
	use tracing_subscriber::layer::SubscriberExt;

	#[rstest]
	fn layer_is_installable_without_otel() {
		// Arrange: build a registry with just our layer — no OTel provider.
		let subscriber = tracing_subscriber::registry().with(TraceContextLogLayer::new());

		// Act: enter a span under the subscriber.
		tracing::subscriber::with_default(subscriber, || {
			let span = tracing::info_span!("test");
			let _enter = span.enter();
		});

		// Assert: completes without panic. Without a real OTel provider the
		// layer simply skips enrichment (no extension inserted).
	}

	#[rstest]
	fn new_constructs_default() {
		// Arrange / Act
		let a = TraceContextLogLayer::new();
		let b = TraceContextLogLayer;

		// Assert
		assert_eq!(format!("{a:?}"), format!("{b:?}"));
	}

	// NOTE: A test asserting that the extension is populated requires a fully
	// initialized `SdkTracerProvider` + `tracing_opentelemetry::layer()`; we
	// exercise that integration path in the operator's test suite rather than
	// here, because `init_tracing` sets a global tracer provider which would
	// interfere with other tests in this crate.
	#[allow(dead_code)] // kept for documentation of the intended invariant
	fn _trace_context_extension_shape() -> TraceContextExtension {
		TraceContextExtension {
			trace_id: "00000000000000000000000000000001".to_string(),
			span_id: "0000000000000001".to_string(),
		}
	}
}
