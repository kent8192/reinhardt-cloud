//! Parse Loki JSON responses into `LogEntry` records.
//!
//! Two response shapes are handled:
//! - `query_range`: `{ "data": { "result": [ { "stream": {...}, "values": [...] } ] } }`
//! - `/tail` (WebSocket frames): `{ "streams": [ { "stream": {...}, "values": [...] } ], "dropped_entries": [] }`

use reinhardt_cloud_types::log::{LogEntry, LogLevel};

/// Extract Loki's `{ "status": "error", "error": "..." }` response.
pub(crate) fn loki_error_message(body: &str) -> Option<String> {
	let root: serde_json::Value = serde_json::from_str(body).ok()?;
	if root.get("status").and_then(|v| v.as_str()) != Some("error") {
		return None;
	}
	Some(
		root.get("error")
			.and_then(|v| v.as_str())
			.unwrap_or("loki returned status=error")
			.to_string(),
	)
}

/// Parse the body of `/loki/api/v1/query_range` into log entries.
pub(crate) fn parse_query_range(body: &str) -> Result<Vec<LogEntry>, serde_json::Error> {
	let root: serde_json::Value = serde_json::from_str(body)?;
	let streams = root
		.get("data")
		.and_then(|d| d.get("result"))
		.and_then(|r| r.as_array());
	let entries = parse_streams(streams)?;
	// `query_range` with `direction=backward` returns newest-first; sort ascending
	// by timestamp for a stable oldest-first dashboard log order. Sorting (rather
	// than a blind reverse) also tolerates streams whose `values` are not strictly
	// pre-sorted by the server.
	let mut entries = entries;
	entries.sort_by_key(|e| e.timestamp);
	Ok(entries)
}

/// Parse a `/loki/api/v1/tail` WebSocket frame into log entries.
///
/// The frame top-level is `{ "streams": [...], "dropped_entries": [...] }`. Each
/// stream has the same `{ "stream": {...}, "values": [[ts, line], ...] }` shape
/// as a `query_range` result entry.
pub(crate) fn parse_tail_frame(body: &str) -> Result<Vec<LogEntry>, serde_json::Error> {
	let root: serde_json::Value = serde_json::from_str(body)?;
	let streams = root.get("streams").and_then(|s| s.as_array());
	parse_streams(streams)
}

/// Shared per-stream parser turning a `values` array into `LogEntry` records.
fn parse_streams(
	streams: Option<&Vec<serde_json::Value>>,
) -> Result<Vec<LogEntry>, serde_json::Error> {
	let mut entries = Vec::new();
	let Some(streams) = streams else {
		return Ok(entries);
	};
	for stream in streams {
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
			let Some(timestamp) = ts_ns.and_then(|ns| {
				chrono::DateTime::from_timestamp(
					ns.div_euclid(1_000_000_000),
					ns.rem_euclid(1_000_000_000) as u32,
				)
			}) else {
				continue;
			};

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
	fn skips_entries_with_malformed_timestamp() {
		// Arrange
		let body = r#"{"data":{"result":[{"stream":{"app":"p"},"values":[["not-nanos","bad"],["1700000000000000000","good"]]}]}}"#;

		// Act
		let entries = parse_query_range(body).unwrap();

		// Assert
		assert_eq!(entries.len(), 1);
		assert_eq!(entries[0].message, "good");
	}

	#[rstest]
	fn extracts_loki_status_error_message() {
		// Arrange
		let body = r#"{"status":"error","error":"parse error at line 1"}"#;

		// Act
		let message = loki_error_message(body);

		// Assert
		assert_eq!(message.as_deref(), Some("parse error at line 1"));
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

		// Assert — sorted ascending by timestamp.
		assert_eq!(
			entries
				.iter()
				.map(|e| e.message.clone())
				.collect::<Vec<_>>(),
			vec!["a".to_string(), "b".to_string(), "c".to_string()]
		);
	}

	#[rstest]
	fn parse_tail_frame_reads_streams_toplevel() {
		// Arrange — /tail frame shape: {"streams":[...],"dropped_entries":[]}
		let body = r#"{"streams":[{"stream":{"app":"p","level":"warn"},"values":[["1700000000000000000","tail-line"]]}],"dropped_entries":[]}"#;

		// Act
		let entries = parse_tail_frame(body).unwrap();

		// Assert
		assert_eq!(entries.len(), 1);
		assert_eq!(entries[0].source, "p");
		assert_eq!(entries[0].level, LogLevel::Warn);
		assert_eq!(entries[0].message, "tail-line");
	}

	#[rstest]
	fn parse_tail_frame_returns_empty_when_no_streams() {
		// Arrange
		let body = r#"{"streams":[],"dropped_entries":[]}"#;

		// Act
		let entries = parse_tail_frame(body).unwrap();

		// Assert
		assert!(entries.is_empty());
	}
}
