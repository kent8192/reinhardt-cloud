//! Manual trace-context helpers.
//!
//! Placeholder stub — populated in a subsequent commit.

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
	None
}
