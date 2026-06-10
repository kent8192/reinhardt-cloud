//! Bridge between [`LogRecord`] and the protobuf `LogEntry`.
//!
//! The protobuf schema (`proto/log.proto`) carries only `timestamp`, `level`,
//! `source`, `message`, and an optional `metadata_json` string. The richer
//! [`LogFields`] (reconcile_id, resource_*, trace_id, ...) are serialised
//! into `metadata_json` as a JSON object so they survive a round-trip.
//!
//! ## Data loss notes
//! - The proto `source` field has no analogue in [`LogRecord`] and is dropped
//!   in both directions (written as empty, ignored on reverse).
//! - `LogLevel::Trace` maps to `ProtoLogLevel::Debug`; the original level is
//!   not recoverable on roundtrip.

use crate::schema::{LogFields, LogLevel, LogRecord};
use chrono::{TimeZone, Utc};
use reinhardt_cloud_proto::log::{LogEntry, LogLevel as ProtoLogLevel};

/// Convert a [`LogRecord`] to its protobuf [`LogEntry`] representation.
pub fn log_record_to_entry(record: &LogRecord) -> LogEntry {
	let metadata_json = fields_to_metadata_json(&record.fields);
	LogEntry {
		timestamp: Some(prost_types::Timestamp {
			seconds: record.ts.timestamp(),
			nanos: record.ts.timestamp_subsec_nanos() as i32,
		}),
		level: level_to_proto(record.level) as i32,
		source: String::new(),
		message: record.msg.clone(),
		metadata_json,
	}
}

/// Convert a protobuf [`LogEntry`] to a [`LogRecord`].
pub fn log_entry_to_record(entry: &LogEntry) -> LogRecord {
	let ts = entry
		.timestamp
		.as_ref()
		.and_then(|t| {
			Utc.timestamp_opt(t.seconds, t.nanos.clamp(0, 999_999_999) as u32)
				.single()
		})
		.unwrap_or_else(Utc::now);
	let level = proto_to_level(ProtoLogLevel::try_from(entry.level).unwrap_or(ProtoLogLevel::Info));
	let fields = entry
		.metadata_json
		.as_deref()
		.and_then(metadata_json_to_fields)
		.unwrap_or_default();

	LogRecord {
		ts,
		level,
		msg: entry.message.clone(),
		fields,
	}
}

fn level_to_proto(level: LogLevel) -> ProtoLogLevel {
	match level {
		// Proto enum has no Trace; map to Debug (closest severity).
		LogLevel::Trace | LogLevel::Debug => ProtoLogLevel::Debug,
		LogLevel::Info => ProtoLogLevel::Info,
		LogLevel::Warn => ProtoLogLevel::Warn,
		LogLevel::Error => ProtoLogLevel::Error,
	}
}

fn proto_to_level(level: ProtoLogLevel) -> LogLevel {
	match level {
		ProtoLogLevel::Unspecified | ProtoLogLevel::Info => LogLevel::Info,
		ProtoLogLevel::Debug => LogLevel::Debug,
		ProtoLogLevel::Warn => LogLevel::Warn,
		ProtoLogLevel::Error => LogLevel::Error,
	}
}

/// Serialise [`LogFields`] into a JSON object string, or `None` if every
/// optional is absent.
fn fields_to_metadata_json(fields: &LogFields) -> Option<String> {
	let value = serde_json::to_value(fields).ok()?;
	match &value {
		serde_json::Value::Object(map) if map.values().all(serde_json::Value::is_null) => None,
		_ => serde_json::to_string(&value).ok(),
	}
}

fn metadata_json_to_fields(json: &str) -> Option<LogFields> {
	serde_json::from_str(json).ok()
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn record_roundtrips_through_proto() {
		// Arrange
		let mut rec = LogRecord::new(LogLevel::Warn, "hello");
		rec.fields.reconcile_id = Some("r-1".into());
		rec.fields.deployment_id = Some("dep-A".into());
		rec.fields.resource_namespace = Some("default".into());

		// Act
		let entry = log_record_to_entry(&rec);
		let back = log_entry_to_record(&entry);

		// Assert
		assert_eq!(back.level, LogLevel::Warn);
		assert_eq!(back.msg, "hello");
		assert_eq!(back.fields.reconcile_id.as_deref(), Some("r-1"));
		assert_eq!(back.fields.deployment_id.as_deref(), Some("dep-A"));
		assert_eq!(back.fields.resource_namespace.as_deref(), Some("default"));
		assert!(back.fields.trace_id.is_none());
	}

	#[rstest]
	fn empty_entry_maps_to_default_fields() {
		// Arrange
		let entry = LogEntry::default();

		// Act
		let rec = log_entry_to_record(&entry);

		// Assert
		assert!(rec.fields.reconcile_id.is_none());
		assert!(rec.fields.deployment_id.is_none());
		assert_eq!(rec.msg, "");
	}

	#[rstest]
	fn timestamp_preserves_nanoseconds() {
		// Arrange
		let ts = chrono::DateTime::<Utc>::from_timestamp(1_700_000_000, 123_456_789).unwrap();
		let rec = LogRecord {
			ts,
			level: LogLevel::Info,
			msg: "ns-check".into(),
			fields: Default::default(),
		};

		// Act
		let entry = log_record_to_entry(&rec);
		let back = log_entry_to_record(&entry);

		// Assert
		assert_eq!(back.ts.timestamp(), 1_700_000_000);
		assert_eq!(back.ts.timestamp_subsec_nanos(), 123_456_789);
	}

	#[rstest]
	fn all_fields_roundtrip_through_proto() {
		// Arrange
		let mut rec = LogRecord::new(LogLevel::Warn, "every field");
		rec.fields.reconcile_id = Some("r".into());
		rec.fields.deployment_id = Some("d".into());
		rec.fields.resource_kind = Some("Project".into());
		rec.fields.resource_namespace = Some("ns".into());
		rec.fields.resource_name = Some("app1".into());
		rec.fields.phase = Some("apply".into());
		rec.fields.correlation_id = Some("corr".into());
		rec.fields.trace_id = Some("t".into());
		rec.fields.span_id = Some("s".into());
		let original_fields = rec.fields.clone();

		// Act
		let entry = log_record_to_entry(&rec);
		let back = log_entry_to_record(&entry);

		// Assert
		assert_eq!(back.fields, original_fields);
	}

	#[rstest]
	fn all_empty_fields_produce_no_metadata_json() {
		// Arrange
		let rec = LogRecord::new(LogLevel::Info, "bare");

		// Act
		let entry = log_record_to_entry(&rec);

		// Assert
		assert!(entry.metadata_json.is_none());
	}
}
