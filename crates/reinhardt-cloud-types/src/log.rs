//! Log streaming domain types for gRPC log services.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Severity level for log entries.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
	Debug,
	Info,
	Warn,
	Error,
}

/// A single log entry from an application or system component.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LogEntry {
	/// Timestamp when the log was generated.
	pub timestamp: DateTime<Utc>,
	/// Severity level.
	pub level: LogLevel,
	/// Source of the log (e.g. app name, pod name, component).
	pub source: String,
	/// Log message content.
	pub message: String,
	/// Optional structured metadata.
	pub metadata: Option<serde_json::Value>,
}

/// Filter criteria for querying or tailing logs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogFilter {
	/// Filter by source (app name, pod name, etc.).
	pub source: Option<String>,
	/// Minimum severity level to include.
	pub min_level: Option<LogLevel>,
	/// Only include logs after this timestamp.
	pub since: Option<DateTime<Utc>>,
	/// Only include logs before this timestamp.
	pub until: Option<DateTime<Utc>>,
	/// Text search within the message field.
	pub search: Option<String>,
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_log_level_ordering() {
		// Assert
		assert!(LogLevel::Debug < LogLevel::Info);
		assert!(LogLevel::Info < LogLevel::Warn);
		assert!(LogLevel::Warn < LogLevel::Error);
	}

	#[rstest]
	fn test_log_entry_serde_roundtrip() {
		// Arrange
		let entry = LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Info,
			source: "my-app/pod-abc".to_string(),
			message: "Request processed in 42ms".to_string(),
			metadata: Some(serde_json::json!({"status": 200, "path": "/api/health"})),
		};

		// Act
		let json = serde_json::to_string(&entry).unwrap();
		let deserialized: LogEntry = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized, entry);
	}

	#[rstest]
	fn test_log_entry_without_metadata() {
		// Arrange
		let entry = LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Error,
			source: "system".to_string(),
			message: "disk full".to_string(),
			metadata: None,
		};

		// Act
		let json = serde_json::to_string(&entry).unwrap();
		let deserialized: LogEntry = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized, entry);
		assert!(deserialized.metadata.is_none());
	}

	#[rstest]
	fn test_log_filter_serde_roundtrip() {
		// Arrange
		let filter = LogFilter {
			source: Some("web-app".to_string()),
			min_level: Some(LogLevel::Warn),
			since: Some(Utc::now()),
			until: None,
			search: Some("error".to_string()),
		};

		// Act
		let json = serde_json::to_string(&filter).unwrap();
		let deserialized: LogFilter = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized.source, Some("web-app".to_string()));
		assert_eq!(deserialized.min_level, Some(LogLevel::Warn));
		assert!(deserialized.until.is_none());
	}

	#[rstest]
	fn test_log_filter_default_is_empty() {
		// Arrange & Act
		let filter = LogFilter::default();

		// Assert
		assert!(filter.source.is_none());
		assert!(filter.min_level.is_none());
		assert!(filter.since.is_none());
		assert!(filter.until.is_none());
		assert!(filter.search.is_none());
	}

	#[rstest]
	fn test_log_level_all_variants_serde() {
		// Arrange
		let levels = vec![
			LogLevel::Debug,
			LogLevel::Info,
			LogLevel::Warn,
			LogLevel::Error,
		];

		// Act & Assert
		for level in &levels {
			let json = serde_json::to_string(level).unwrap();
			let deserialized: LogLevel = serde_json::from_str(&json).unwrap();
			assert_eq!(&deserialized, level);
		}
	}

	#[rstest]
	fn test_log_entry_metadata_array() {
		// Arrange
		let entry = LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Info,
			source: "test-source".to_string(),
			message: "array metadata".to_string(),
			metadata: Some(serde_json::json!([1, 2, 3])),
		};

		// Act
		let json = serde_json::to_string(&entry).unwrap();
		let deserialized: LogEntry = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized, entry);
		assert_eq!(
			deserialized.metadata,
			Some(serde_json::json!([1, 2, 3]))
		);
	}

	#[rstest]
	fn test_log_entry_very_large_metadata() {
		// Arrange
		let nested = serde_json::json!({
			"level1": {
				"level2": {
					"level3": {
						"level4": {
							"level5": {
								"data": [1, 2, 3, 4, 5],
								"flag": true,
								"name": "deeply nested"
							}
						}
					}
				}
			}
		});
		let entry = LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Debug,
			source: "deep-source".to_string(),
			message: "deeply nested metadata".to_string(),
			metadata: Some(nested.clone()),
		};

		// Act
		let json = serde_json::to_string(&entry).unwrap();
		let deserialized: LogEntry = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized, entry);
		assert_eq!(deserialized.metadata, Some(nested));
	}

	#[rstest]
	fn test_log_filter_since_after_until() {
		// Arrange — since > until is structurally valid (no validation)
		let now = Utc::now();
		let past = now - chrono::Duration::hours(1);
		let filter = LogFilter {
			source: None,
			min_level: None,
			since: Some(now),
			until: Some(past),
			search: None,
		};

		// Act
		let json = serde_json::to_string(&filter).unwrap();
		let deserialized: LogFilter = serde_json::from_str(&json).unwrap();

		// Assert
		assert!(deserialized.since.unwrap() > deserialized.until.unwrap());
	}

	#[rstest]
	fn test_log_level_same_variant_equality() {
		// Arrange
		let a = LogLevel::Debug;
		let b = LogLevel::Debug;

		// Act & Assert
		assert_eq!(a, b);
	}

	#[rstest]
	#[case(Some("web-app"), None, None)]
	#[case(None, Some(LogLevel::Warn), None)]
	#[case(None, None, Some("error pattern"))]
	#[case(Some("api"), Some(LogLevel::Error), Some("timeout"))]
	#[case(None, None, None)]
	#[case(Some("worker"), Some(LogLevel::Debug), None)]
	fn test_log_filter_all_field_combinations(
		#[case] source: Option<&str>,
		#[case] min_level: Option<LogLevel>,
		#[case] search: Option<&str>,
	) {
		// Arrange
		let filter = LogFilter {
			source: source.map(String::from),
			min_level,
			since: None,
			until: None,
			search: search.map(String::from),
		};

		// Act
		let json = serde_json::to_string(&filter).unwrap();
		let deserialized: LogFilter = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized.source, source.map(String::from));
		assert_eq!(deserialized.min_level, min_level);
		assert_eq!(deserialized.search, search.map(String::from));
	}

	mod proptest_log {
		use super::*;
		use proptest::prelude::*;

		proptest! {
			#[test]
			fn prop_log_level_ordering_total(a_idx in 0..4u8, b_idx in 0..4u8) {
				let levels = [LogLevel::Debug, LogLevel::Info, LogLevel::Warn, LogLevel::Error];
				let a = levels[a_idx as usize];
				let b = levels[b_idx as usize];
				// Exactly one of: a < b, a == b, a > b
				let lt = a < b;
				let eq = a == b;
				let gt = a > b;
				prop_assert_eq!(lt as u8 + eq as u8 + gt as u8, 1);
			}

			#[test]
			fn prop_log_entry_serde_idempotent(
				level_idx in 0..4u8,
				source in "[a-z]{1,20}",
				message in "\\PC{0,100}",
			) {
				let levels = [LogLevel::Debug, LogLevel::Info, LogLevel::Warn, LogLevel::Error];
				let entry = LogEntry {
					timestamp: Utc::now(),
					level: levels[level_idx as usize],
					source,
					message,
					metadata: None,
				};
				let json1 = serde_json::to_string(&entry).unwrap();
				let roundtrip: LogEntry = serde_json::from_str(&json1).unwrap();
				let json2 = serde_json::to_string(&roundtrip).unwrap();
				prop_assert_eq!(json1, json2);
			}

			#[test]
			fn fuzz_log_entry_deserialize_no_panic(s in "\\PC*") {
				let _ = serde_json::from_str::<LogEntry>(&s);
			}
		}
	}
}
