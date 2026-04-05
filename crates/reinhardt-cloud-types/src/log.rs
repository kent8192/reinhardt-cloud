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
}
