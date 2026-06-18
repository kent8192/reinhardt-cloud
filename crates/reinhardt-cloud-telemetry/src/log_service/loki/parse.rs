//! Parse a Loki `query_range` / `tail` JSON response into `LogEntry` records.

use reinhardt_cloud_types::log::{LogEntry, LogLevel};

/// Parse the body of `/loki/api/v1/query_range` (or a `/tail` frame) into log
/// entries.
///
/// Loki shape: `{ "data": { "result": [ { "stream": {...}, "values": [ [tsNs, line], ... ] } ] } }`.
/// Each `line` may itself be a JSON object with `level`/`msg`; otherwise the
/// whole line is treated as the message.
pub fn parse_query_range(body: &str) -> Result<Vec<LogEntry>, serde_json::Error> {
	let root: serde_json::Value = serde_json::from_str(body)?;
	let mut entries = Vec::new();

	let Some(result) = root
		.get("data")
		.and_then(|d| d.get("result"))
		.and_then(|r| r.as_array())
	else {
		return Ok(entries);
	};

	for stream in result {
		let labels = stream.get("stream").and_then(|s| s.as_object());
		let Some(values) = stream.get("values").and_then(|v| v.as_array()) else {
			continue;
		};
		for pair in values {
			let mut iter = pair.as_array().into_iter().flatten();
			let ts_ns = iter
				.next()
				.and_then(|v| v.as_str())
				.and_then(|s| s.parse::<i64>().ok());
			let line = iter.next().and_then(|v| v.as_str()).unwrap_or("");
			let timestamp = ts_ns
				.and_then(|ns| {
					chrono::DateTime::from_timestamp(ns / 1_000_000_000, (ns % 1_000_000_000) as u32)
				})
				.unwrap_or_else(chrono::Utc::now);

			let (level, source, message, metadata) = split_line(line, labels);
			entries.push(LogEntry {
				timestamp,
				level,
				source,
				message,
				metadata,
			});
		}
	}

	// `query_range` with `direction=backward` returns newest-first; sort ascending
	// by timestamp for a stable oldest-first dashboard log order. Sorting (rather
	// than a blind reverse) also tolerates streams whose `values` are not strictly
	// pre-sorted by the server.
	entries.sort_by_key(|e| e.timestamp);
	Ok(entries)
}

/// Split a Loki log line into (level, source, message, metadata).
///
/// If `line` is a JSON object, pull structured fields out; otherwise treat the
/// whole line as the message and derive level/source from the stream labels.
fn split_line(
	line: &str,
	labels: Option<&serde_json::Map<String, serde_json::Value>>,
) -> (LogLevel, String, String, Option<serde_json::Value>) {
	let label = |key: &str| {
		labels
			.and_then(|m| m.get(key))
			.and_then(|v| v.as_str())
			.unwrap_or("")
			.to_string()
	};
	match serde_json::from_str::<serde_json::Value>(line) {
		Ok(obj @ serde_json::Value::Object(_)) => {
			let m = obj.as_object().unwrap();
			let level = m
				.get("level")
				.and_then(|v| v.as_str())
				.and_then(parse_level)
				.unwrap_or_else(|| parse_level(&label("level")).unwrap_or(LogLevel::Info));
			let source = m
				.get("source")
				.and_then(|v| v.as_str())
				.map(|s| s.to_string())
				.unwrap_or_else(|| label("app"));
			let message = m
				.get("msg")
				.or_else(|| m.get("message"))
				.and_then(|v| v.as_str())
				.unwrap_or(line)
				.to_string();
			(level, source, message, Some(obj))
		}
		_ => {
			let level = parse_level(&label("level")).unwrap_or(LogLevel::Info);
			let source = label("app");
			(level, source, line.to_string(), None)
		}
	}
}

/// Parse a level string (case-insensitive) into a `LogLevel`.
fn parse_level(s: &str) -> Option<LogLevel> {
	match s.to_ascii_lowercase().as_str() {
		"debug" | "trace" => Some(LogLevel::Debug),
		"info" => Some(LogLevel::Info),
		"warn" | "warning" => Some(LogLevel::Warn),
		"error" | "err" => Some(LogLevel::Error),
		_ => None,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn parses_single_json_line_with_level_and_msg() {
		// Arrange
		let body = r#"{"data":{"result":[{"stream":{"app":"p","level":"info"},"values":[["1700000000000000000","{\"level\":\"warn\",\"msg\":\"hi\"}"]]}]}}"#;

		// Act
		let entries = parse_query_range(body).unwrap();

		// Assert — JSON line wins over stream label for level.
		assert_eq!(entries.len(), 1);
		assert_eq!(entries[0].level, LogLevel::Warn);
		assert_eq!(entries[0].message, "hi");
	}

	#[rstest]
	fn parses_plain_line_using_stream_labels() {
		// Arrange
		let body = r#"{"data":{"result":[{"stream":{"app":"svc","level":"error"},"values":[["1700000000000000000","disk full"]]}]}}"#;

		// Act
		let entries = parse_query_range(body).unwrap();

		// Assert
		assert_eq!(entries.len(), 1);
		assert_eq!(entries[0].level, LogLevel::Error);
		assert_eq!(entries[0].source, "svc");
		assert_eq!(entries[0].message, "disk full");
		assert!(entries[0].metadata.is_none());
	}

	#[rstest]
	fn returns_empty_for_missing_result_array() {
		// Arrange
		let body = r#"{"data":{}}"#;

		// Act
		let entries = parse_query_range(body).unwrap();

		// Assert
		assert!(entries.is_empty());
	}

	#[rstest]
	fn newest_first_input_is_returned_oldest_first() {
		// Arrange — Loki `direction=backward` gives newest-first.
		let body = r#"{"data":{"result":[{"stream":{"app":"p"},"values":[
			["1700000003000000000","c"],
			["1700000001000000000","a"],
			["1700000002000000000","b"]
		]}]}}"#;

		// Act
		let entries = parse_query_range(body).unwrap();

		// Assert — reversed to oldest-first.
		assert_eq!(
			entries.iter().map(|e| e.message.clone()).collect::<Vec<_>>(),
			vec!["a".to_string(), "b".to_string(), "c".to_string()]
		);
	}
}
