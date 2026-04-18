//! Log schema types (see design doc §3).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Severity of a log record. Ordered from least to most severe.
///
/// The declaration order (Trace < Debug < Info < Warn < Error) defines the
/// total ordering used for level-based filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
	Trace,
	Debug,
	Info,
	Warn,
	Error,
}

/// Optional correlation and resource fields attached to a [`LogRecord`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogFields {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub reconcile_id: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub resource_kind: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub resource_namespace: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub resource_name: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub phase: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub correlation_id: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub deployment_id: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub trace_id: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub span_id: Option<String>,
}

/// A structured log record produced by reinhardt-cloud components.
///
/// The [`LogFields`] are flattened into the serialized representation so
/// that consumers (Loki, JSON log processors) see a flat key/value shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogRecord {
	pub ts: DateTime<Utc>,
	pub level: LogLevel,
	pub msg: String,
	#[serde(flatten)]
	pub fields: LogFields,
}

impl LogRecord {
	/// Construct a new record timestamped at `Utc::now()` with no correlation fields.
	pub fn new(level: LogLevel, msg: impl Into<String>) -> Self {
		Self {
			ts: Utc::now(),
			level,
			msg: msg.into(),
			fields: LogFields::default(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn log_record_serializes_flat_fields() {
		// Arrange
		let mut rec = LogRecord::new(LogLevel::Info, "hello");
		rec.fields.reconcile_id = Some("r-42".into());

		// Act
		let json = serde_json::to_value(&rec).unwrap();

		// Assert: fields are flattened, not nested under "fields"
		assert_eq!(json["msg"], "hello");
		assert_eq!(json["level"], "info");
		assert_eq!(json["reconcile_id"], "r-42");
		assert!(json.get("fields").is_none());
	}

	#[rstest]
	fn log_record_roundtrips_through_json() {
		// Arrange
		let original = LogRecord::new(LogLevel::Error, "boom");

		// Act
		let json = serde_json::to_string(&original).unwrap();
		let parsed: LogRecord = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(parsed, original);
	}
}
