//! SSE (Server-Sent Events) adapter for gRPC streams.
//!
//! Converts gRPC streaming responses into SSE-compatible event streams
//! for consumption by control-plane clients.

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

	#[rstest]
	#[case(
		BuildEvent::Log { message: "step 1".to_string(), timestamp: Utc::now() },
		"build_log"
	)]
	#[case(
		BuildEvent::PhaseChange { phase: BuildPhase::Queued, timestamp: Utc::now() },
		"build_phase"
	)]
	#[case(
		BuildEvent::ArtifactReady {
			artifact_url: "url".to_string(),
			digest: "sha256:aa".to_string(),
			timestamp: Utc::now(),
		},
		"build_artifact"
	)]
	#[case(
		BuildEvent::Error { message: "fail".to_string(), timestamp: Utc::now() },
		"build_error"
	)]
	#[case(
		BuildEvent::Complete { success: true, timestamp: Utc::now() },
		"build_complete"
	)]
	fn test_build_event_to_sse_all_5_variants(
		#[case] event: BuildEvent,
		#[case] expected_type: &str,
	) {
		// Arrange — provided by #[case]

		// Act
		let sse = build_event_to_sse(&event).unwrap();

		// Assert
		assert_eq!(sse.event_type, expected_type);
		// Verify valid JSON in data field
		let _: serde_json::Value = serde_json::from_str(&sse.data).unwrap();
	}

	#[rstest]
	fn test_log_entry_to_sse_with_metadata() {
		// Arrange
		let entry = LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Info,
			source: "api".to_string(),
			message: "request handled".to_string(),
			metadata: Some(serde_json::json!({"status": 200, "latency_ms": 42})),
		};

		// Act
		let sse = log_entry_to_sse(&entry).unwrap();

		// Assert
		assert_eq!(sse.event_type, "log");
		assert!(sse.data.contains("request handled"));
		assert!(sse.data.contains("200"));
		assert!(sse.data.contains("42"));
	}

	#[rstest]
	fn test_sse_event_unicode_data() {
		// Arrange
		let event = SseEvent {
			event_type: "test".to_string(),
			data: r#"{"msg":"Hello \u4e16\u754c \u2603 \u2764"}"#.to_string(),
		};

		// Act
		let sse_str = event.to_sse_string();

		// Assert
		assert!(sse_str.starts_with("event: test\n"));
		assert!(sse_str.contains(r#"\u4e16\u754c"#));
		assert!(sse_str.ends_with("\n\n"));
	}

	#[rstest]
	fn test_sse_event_unicode_in_build_event() {
		// Arrange — log message with emoji and CJK characters
		let event = BuildEvent::Log {
			message: "Build \u{2705} \u{5b8c}\u{6210}".to_string(),
			timestamp: Utc::now(),
		};

		// Act
		let sse = build_event_to_sse(&event).unwrap();

		// Assert
		assert_eq!(sse.event_type, "build_log");
		assert!(sse.data.contains("\u{2705}"));
		assert!(sse.data.contains("\u{5b8c}\u{6210}"));
	}
}
