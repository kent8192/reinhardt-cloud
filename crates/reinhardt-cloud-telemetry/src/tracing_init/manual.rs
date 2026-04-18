//! Manual trace-context helpers.
//!
//! Exposes a read-only accessor for the currently active OpenTelemetry trace
//! context. Useful for enriching ad-hoc log lines or propagating `traceparent`
//! headers outside of instrumented HTTP/gRPC clients.

use opentelemetry::trace::TraceContextExt;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// Trace context snapshot for the currently active span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceContext {
	/// 32-hex-char OpenTelemetry trace ID.
	pub trace_id: String,
	/// 16-hex-char OpenTelemetry span ID.
	pub span_id: String,
}

/// Return the current OpenTelemetry trace context, if any.
///
/// Returns `None` when no OpenTelemetry layer is installed, when there is no
/// active span, or when the active span does not carry a valid trace context.
pub fn current_trace_context() -> Option<TraceContext> {
	let otel_cx = Span::current().context();
	let otel_span = otel_cx.span();
	let span_ctx = otel_span.span_context();
	if !span_ctx.is_valid() {
		return None;
	}
	Some(TraceContext {
		trace_id: span_ctx.trace_id().to_string(),
		span_id: span_ctx.span_id().to_string(),
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn returns_none_outside_any_span() {
		// Arrange: no subscriber, no OTel provider, no active span.
		// Act
		let ctx = current_trace_context();

		// Assert
		assert_eq!(ctx, None);
	}
}
