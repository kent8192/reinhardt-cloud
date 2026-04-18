//! Log schema types (see design doc §3).

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
	Trace,
	Debug,
	Info,
	Warn,
	Error,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogFields {
	pub reconcile_id: Option<String>,
	pub resource_kind: Option<String>,
	pub resource_namespace: Option<String>,
	pub resource_name: Option<String>,
	pub phase: Option<String>,
	pub correlation_id: Option<String>,
	pub trace_id: Option<String>,
	pub span_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogRecord {
	#[serde(with = "time::serde::rfc3339")]
	pub ts: OffsetDateTime,
	pub level: LogLevel,
	pub msg: String,
	#[serde(flatten)]
	pub fields: LogFields,
}

impl LogRecord {
	pub fn new(level: LogLevel, msg: impl Into<String>) -> Self {
		Self {
			ts: OffsetDateTime::now_utc(),
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
		assert_eq!(parsed.level, LogLevel::Error);
		assert_eq!(parsed.msg, "boom");
	}
}
