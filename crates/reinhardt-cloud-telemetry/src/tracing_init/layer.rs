//! Tracing subscriber layer bridging OpenTelemetry span context to log records.
//!
//! On the first `enter` of a span, this layer reads the `OtelData` extension
//! populated by `tracing_opentelemetry::OpenTelemetryLayer` and caches the
//! resulting 32-hex `trace_id` and 16-hex `span_id` as a
//! [`TraceContextExtension`] on the span so downstream fmt layers / log
//! enrichers can correlate logs with traces.
//!
//! # Layer ordering requirement
//!
//! This layer depends on `tracing_opentelemetry::OpenTelemetryLayer` having
//! transitioned its `OtelData` extension into the `Context` state by the time
//! our `on_enter` runs. That happens inside the OTel layer's own `on_enter`.
//! `tracing-subscriber` invokes per-span callbacks in layer registration
//! order (inner first), so the OTel layer MUST be registered BEFORE
//! `TraceContextLogLayer` in the `Registry::with(...)` chain.

use tracing::Id;
use tracing_opentelemetry::OtelData;
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
	fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
		let Some(span_ref) = ctx.span(id) else {
			return;
		};

		// On the first `enter`, `OpenTelemetryLayer::on_enter` (which ran
		// immediately before us because it was registered first) has moved
		// `OtelData` from its `Builder` state into `Context` state. That is
		// the earliest point at which `OtelData::trace_id()` and `span_id()`
		// return `Some`.
		let mut extensions = span_ref.extensions_mut();
		if extensions.get_mut::<TraceContextExtension>().is_some() {
			return;
		}

		let Some(otel_data) = extensions.get_mut::<OtelData>() else {
			return;
		};
		let (Some(trace_id), Some(span_id)) = (otel_data.trace_id(), otel_data.span_id()) else {
			return;
		};

		extensions.insert(TraceContextExtension {
			trace_id: trace_id.to_string(),
			span_id: span_id.to_string(),
		});
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
}
