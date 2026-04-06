//! SSE (Server-Sent Events) adapter for gRPC streams.
//!
//! Converts gRPC streaming responses into SSE-compatible event streams
//! for consumption by the CLI client via REST API endpoints.

use serde::Serialize;

/// An SSE event ready for serialization.
#[derive(Debug, Clone)]
pub struct SseEvent {
	/// Event type (used as SSE `event:` field).
	pub event_type: String,
	/// JSON-serialized data.
	pub data: String,
}

impl SseEvent {
	/// Create a new SSE event from a serializable payload.
	pub fn new<T: Serialize>(event_type: &str, payload: &T) -> Result<Self, serde_json::Error> {
		Ok(Self {
			event_type: event_type.to_string(),
			data: serde_json::to_string(payload)?,
		})
	}

	/// Format as an SSE message string.
	pub fn to_sse_string(&self) -> String {
		format!("event: {}\ndata: {}\n\n", self.event_type, self.data)
	}
}

/// Convert a `BuildEvent` to an SSE event.
pub fn build_event_to_sse(
	event: &reinhardt_cloud_types::build::BuildEvent,
) -> Result<SseEvent, serde_json::Error> {
	use reinhardt_cloud_types::build::BuildEvent;
	let event_type = match event {
		BuildEvent::Log { .. } => "build_log",
		BuildEvent::PhaseChange { .. } => "build_phase",
		BuildEvent::ArtifactReady { .. } => "build_artifact",
		BuildEvent::Error { .. } => "build_error",
		BuildEvent::Complete { .. } => "build_complete",
	};
	SseEvent::new(event_type, event)
}

/// Convert a `LogEntry` to an SSE event.
pub fn log_entry_to_sse(
	entry: &reinhardt_cloud_types::log::LogEntry,
) -> Result<SseEvent, serde_json::Error> {
	SseEvent::new("log", entry)
}

#[cfg(test)]
mod tests {
	use super::*;
	use chrono::Utc;
	use reinhardt_cloud_types::build::{BuildEvent, BuildPhase};
	use reinhardt_cloud_types::log::{LogEntry, LogLevel};
	use rstest::rstest;

	#[rstest]
	fn test_sse_event_format() {
		// Arrange
		let event = SseEvent {
			event_type: "test".to_string(),
			data: r#"{"key":"value"}"#.to_string(),
		};

		// Act
		let sse = event.to_sse_string();

		// Assert
		assert!(sse.starts_with("event: test\n"));
		assert!(sse.contains("data: {\"key\":\"value\"}"));
		assert!(sse.ends_with("\n\n"));
	}

	#[rstest]
	fn test_build_event_to_sse() {
		// Arrange
		let event = BuildEvent::PhaseChange {
			phase: BuildPhase::Building,
			timestamp: Utc::now(),
		};

		// Act
		let sse = build_event_to_sse(&event).unwrap();

		// Assert
		assert_eq!(sse.event_type, "build_phase");
		assert!(sse.data.contains("Building"));
	}

	#[rstest]
	fn test_log_entry_to_sse() {
		// Arrange
		let entry = LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Error,
			source: "web".to_string(),
			message: "connection refused".to_string(),
			metadata: None,
		};

		// Act
		let sse = log_entry_to_sse(&entry).unwrap();

		// Assert
		assert_eq!(sse.event_type, "log");
		assert!(sse.data.contains("connection refused"));
	}
}
