//! Tracing subscriber layer bridging OpenTelemetry span context to log records.
//!
//! Placeholder stub — populated in a subsequent commit.

use tracing_subscriber::Layer;
use tracing_subscriber::registry::LookupSpan;

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
	// Default no-op; enriched in a later commit.
}
