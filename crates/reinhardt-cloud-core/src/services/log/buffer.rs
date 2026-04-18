//! In-memory ring buffer for log entries.

use std::collections::VecDeque;

use tokio::sync::{Mutex, broadcast};

use reinhardt_cloud_types::log::{LogEntry, LogFilter};

/// Default capacity for the log ring buffer.
const DEFAULT_CAPACITY: usize = 10_000;

/// In-memory ring buffer for log entries with fan-out support.
///
/// Stores entries in a `VecDeque` (ring buffer) and broadcasts
/// new entries to active `TailLogs` subscribers.
pub struct LogBuffer {
	entries: Mutex<VecDeque<LogEntry>>,
	capacity: usize,
	broadcast_tx: broadcast::Sender<LogEntry>,
}

impl LogBuffer {
	/// Create a new log buffer with the given capacity.
	pub fn new(capacity: usize) -> Self {
		let (broadcast_tx, _) = broadcast::channel(1024);
		Self {
			entries: Mutex::new(VecDeque::with_capacity(capacity)),
			capacity,
			broadcast_tx,
		}
	}

	/// Push log entries into the buffer.
	///
	/// Drops oldest entries if capacity is exceeded.
	/// Broadcasts each entry to active subscribers.
	pub async fn push(&self, entries: Vec<LogEntry>) {
		let mut buf = self.entries.lock().await;
		for entry in entries {
			// Broadcast to subscribers (ignore send errors — no receivers)
			let _ = self.broadcast_tx.send(entry.clone());

			buf.push_back(entry);
			if buf.len() > self.capacity {
				buf.pop_front();
			}
		}
	}

	/// Subscribe to new log entries matching the filter.
	///
	/// Returns a broadcast receiver that will receive new entries.
	pub fn subscribe(&self) -> broadcast::Receiver<LogEntry> {
		self.broadcast_tx.subscribe()
	}

	/// Query stored entries matching the filter.
	pub async fn query(&self, filter: &LogFilter) -> Vec<LogEntry> {
		let buf = self.entries.lock().await;
		buf.iter()
			.filter(|entry| matches_filter(entry, filter))
			.cloned()
			.collect()
	}

	/// Total number of entries currently in the buffer.
	pub async fn len(&self) -> usize {
		self.entries.lock().await.len()
	}

	/// Returns true if the buffer contains no entries.
	pub async fn is_empty(&self) -> bool {
		self.entries.lock().await.is_empty()
	}
}

impl Default for LogBuffer {
	fn default() -> Self {
		Self::new(DEFAULT_CAPACITY)
	}
}

/// Check if a log entry matches the given filter.
pub fn matches_filter(entry: &LogEntry, filter: &LogFilter) -> bool {
	if let Some(source) = &filter.source
		&& !entry.source.contains(source.as_str())
	{
		return false;
	}
	if let Some(min_level) = &filter.min_level
		&& entry.level < *min_level
	{
		return false;
	}
	if let Some(since) = &filter.since
		&& entry.timestamp < *since
	{
		return false;
	}
	if let Some(until) = &filter.until
		&& entry.timestamp > *until
	{
		return false;
	}
	if let Some(search) = &filter.search
		&& !entry.message.contains(search.as_str())
	{
		return false;
	}
	if let Some(deployment_id) = &filter.deployment_id
		&& entry.source.as_str() != deployment_id.as_str()
	{
		return false;
	}
	true
}

#[cfg(test)]
mod tests {
	use super::*;
	use chrono::Utc;
	use reinhardt_cloud_types::log::LogLevel;
	use rstest::rstest;

	fn make_entry(source: &str, level: LogLevel, msg: &str) -> LogEntry {
		LogEntry {
			timestamp: Utc::now(),
			level,
			source: source.to_string(),
			message: msg.to_string(),
			metadata: None,
		}
	}

	#[rstest]
	#[tokio::test]
	async fn test_push_and_query() {
		// Arrange
		let buffer = LogBuffer::new(100);
		let entries = vec![
			make_entry("app", LogLevel::Info, "hello"),
			make_entry("app", LogLevel::Error, "oops"),
		];

		// Act
		buffer.push(entries).await;
		let all = buffer.query(&LogFilter::default()).await;

		// Assert
		assert_eq!(all.len(), 2);
	}

	#[rstest]
	#[tokio::test]
	async fn test_ring_buffer_overflow() {
		// Arrange
		let buffer = LogBuffer::new(3);

		// Act — push 5 entries into buffer of size 3
		for i in 0..5 {
			buffer
				.push(vec![make_entry("app", LogLevel::Info, &format!("msg-{i}"))])
				.await;
		}

		// Assert — only last 3 remain
		assert_eq!(buffer.len().await, 3);
		let entries = buffer.query(&LogFilter::default()).await;
		assert_eq!(entries[0].message, "msg-2");
		assert_eq!(entries[2].message, "msg-4");
	}

	#[rstest]
	#[tokio::test]
	async fn test_filter_by_level() {
		// Arrange
		let buffer = LogBuffer::new(100);
		buffer
			.push(vec![
				make_entry("app", LogLevel::Debug, "debug"),
				make_entry("app", LogLevel::Info, "info"),
				make_entry("app", LogLevel::Error, "error"),
			])
			.await;

		// Act
		let filter = LogFilter {
			min_level: Some(LogLevel::Warn),
			..Default::default()
		};
		let result = buffer.query(&filter).await;

		// Assert
		assert_eq!(result.len(), 1);
		assert_eq!(result[0].level, LogLevel::Error);
	}

	#[rstest]
	#[tokio::test]
	async fn test_filter_by_source() {
		// Arrange
		let buffer = LogBuffer::new(100);
		buffer
			.push(vec![
				make_entry("web-app", LogLevel::Info, "req"),
				make_entry("worker", LogLevel::Info, "job"),
			])
			.await;

		// Act
		let filter = LogFilter {
			source: Some("worker".to_string()),
			..Default::default()
		};
		let result = buffer.query(&filter).await;

		// Assert
		assert_eq!(result.len(), 1);
		assert_eq!(result[0].source, "worker");
	}

	#[rstest]
	#[tokio::test]
	async fn test_filter_by_search() {
		// Arrange
		let buffer = LogBuffer::new(100);
		buffer
			.push(vec![
				make_entry("app", LogLevel::Info, "request processed"),
				make_entry("app", LogLevel::Error, "connection timeout"),
			])
			.await;

		// Act
		let filter = LogFilter {
			search: Some("timeout".to_string()),
			..Default::default()
		};
		let result = buffer.query(&filter).await;

		// Assert
		assert_eq!(result.len(), 1);
		assert!(result[0].message.contains("timeout"));
	}

	#[rstest]
	#[tokio::test]
	async fn test_filter_by_deployment_id() {
		// Arrange
		let buffer = LogBuffer::new(100);
		buffer
			.push(vec![
				make_entry("deploy-a", LogLevel::Info, "from a"),
				make_entry("deploy-b", LogLevel::Info, "from b"),
				make_entry("deploy-a", LogLevel::Error, "from a again"),
			])
			.await;

		// Act
		let filter = LogFilter {
			deployment_id: Some("deploy-a".to_string()),
			..Default::default()
		};
		let result = buffer.query(&filter).await;

		// Assert — only entries whose source matches the deployment_id are included
		assert_eq!(result.len(), 2);
		assert!(result.iter().all(|e| e.source == "deploy-a"));
	}

	#[rstest]
	#[tokio::test]
	async fn test_broadcast_subscriber() {
		// Arrange
		let buffer = LogBuffer::new(100);
		let mut rx = buffer.subscribe();

		// Act
		buffer
			.push(vec![make_entry("app", LogLevel::Info, "hello")])
			.await;

		// Assert
		let received = rx.recv().await.unwrap();
		assert_eq!(received.message, "hello");
	}
}
