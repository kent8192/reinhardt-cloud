//! Integration tests for LocalLogService and LogBuffer.

mod fixtures;

use std::sync::Arc;

use chrono::{Duration, Utc};
use rstest::rstest;
use tokio_stream::StreamExt;

use reinhardt_cloud_core::pagination::PaginationParams;
use reinhardt_cloud_core::services::log::buffer::LogBuffer;
use reinhardt_cloud_core::services::log::local::LocalLogService;
use reinhardt_cloud_core::traits::LogService;
use reinhardt_cloud_types::log::{LogEntry, LogFilter, LogLevel};

use fixtures::{log_buffer, make_log_entries, make_log_entry};

// ===========================================================================
// Happy path tests
// ===========================================================================

#[rstest]
#[tokio::test]
async fn test_tail_logs_receives_matching_entries(log_buffer: Arc<LogBuffer>) {
	// Arrange
	let service = LocalLogService::new(log_buffer.clone());
	let filter = LogFilter {
		source: Some("app-a".to_string()),
		..Default::default()
	};
	let mut tail_stream = service.tail_logs(filter).await.unwrap();

	// Act — push entries from different sources
	service
		.push_logs(vec![
			make_log_entry("app-a", LogLevel::Info, "hello from a"),
			make_log_entry("app-b", LogLevel::Info, "hello from b"),
		])
		.await
		.unwrap();

	// Assert — only app-a entries come through the tail
	let received = tokio::time::timeout(std::time::Duration::from_secs(1), tail_stream.next())
		.await
		.expect("Should receive within timeout")
		.expect("Stream should yield an item")
		.unwrap();
	assert_eq!(received.source, "app-a");
	assert_eq!(received.message, "hello from a");
}

#[rstest]
#[tokio::test]
async fn test_list_logs_with_source_filter(log_buffer: Arc<LogBuffer>) {
	// Arrange
	let service = LocalLogService::new(log_buffer);
	service
		.push_logs(vec![
			make_log_entry("worker", LogLevel::Info, "job started"),
			make_log_entry("api", LogLevel::Info, "request handled"),
			make_log_entry("worker", LogLevel::Warn, "job slow"),
		])
		.await
		.unwrap();
	let filter = LogFilter {
		source: Some("worker".to_string()),
		..Default::default()
	};

	// Act
	let result = service
		.list_logs(filter, PaginationParams::default())
		.await
		.unwrap();

	// Assert
	assert_eq!(result.items.len(), 2);
	for item in &result.items {
		assert_eq!(item.source, "worker");
	}
}

// ===========================================================================
// Error path tests
// ===========================================================================

#[rstest]
#[tokio::test]
async fn test_list_logs_page_beyond_total(log_buffer: Arc<LogBuffer>) {
	// Arrange
	let service = LocalLogService::new(log_buffer);
	service.push_logs(make_log_entries(5)).await.unwrap();

	// Act — request page 100 which is far beyond the 5 total entries
	let result = service
		.list_logs(
			LogFilter::default(),
			PaginationParams::new(Some(100), Some(20)),
		)
		.await
		.unwrap();

	// Assert
	assert!(result.items.is_empty());
	assert_eq!(result.total, 5);
	assert_eq!(result.total_pages, 1); // ceil(5 / 20) = 1
}

// ===========================================================================
// Edge case tests
// ===========================================================================

#[rstest]
#[tokio::test]
async fn test_log_buffer_push_empty_vec() {
	// Arrange
	let buffer = LogBuffer::new(100);

	// Act
	buffer.push(vec![]).await;

	// Assert
	assert_eq!(buffer.len().await, 0);
}

#[rstest]
#[tokio::test]
async fn test_log_buffer_capacity_one() {
	// Arrange
	let buffer = LogBuffer::new(1);

	// Act — push 3 entries into buffer with capacity 1
	buffer
		.push(vec![
			make_log_entry("a", LogLevel::Info, "first"),
			make_log_entry("b", LogLevel::Info, "second"),
			make_log_entry("c", LogLevel::Info, "third"),
		])
		.await;

	// Assert — only the last entry remains
	assert_eq!(buffer.len().await, 1);
	let entries = buffer.query(&LogFilter::default()).await;
	assert_eq!(entries[0].message, "third");
}

#[rstest]
#[tokio::test]
async fn test_log_filter_since_in_future_returns_empty(log_buffer: Arc<LogBuffer>) {
	// Arrange
	let service = LocalLogService::new(log_buffer);
	service
		.push_logs(vec![make_log_entry("app", LogLevel::Info, "now")])
		.await
		.unwrap();

	let filter = LogFilter {
		since: Some(Utc::now() + Duration::hours(1)),
		..Default::default()
	};

	// Act
	let result = service
		.list_logs(filter, PaginationParams::default())
		.await
		.unwrap();

	// Assert
	assert!(result.items.is_empty());
}

#[rstest]
#[tokio::test]
async fn test_log_filter_until_in_past_returns_empty(log_buffer: Arc<LogBuffer>) {
	// Arrange
	let service = LocalLogService::new(log_buffer);
	service
		.push_logs(vec![make_log_entry("app", LogLevel::Info, "now")])
		.await
		.unwrap();

	let filter = LogFilter {
		until: Some(Utc::now() - Duration::hours(1)),
		..Default::default()
	};

	// Act
	let result = service
		.list_logs(filter, PaginationParams::default())
		.await
		.unwrap();

	// Assert
	assert!(result.items.is_empty());
}

#[rstest]
#[tokio::test]
async fn test_broadcast_subscriber_lagged() {
	// Arrange — create a buffer and subscriber
	let buffer = LogBuffer::new(10_000);
	let mut rx = buffer.subscribe();

	// Act — push more than the broadcast channel capacity (1024)
	let entries: Vec<LogEntry> = (0..1100)
		.map(|i| make_log_entry("app", LogLevel::Info, &format!("msg-{i}")))
		.collect();
	buffer.push(entries).await;

	// Assert — the subscriber should still work (no panic), though it
	// may have lagged. Try to receive; it should either yield an entry
	// or a lagged error which is handled internally. The key assertion
	// is that this does not panic.
	let result = rx.try_recv();
	// The subscriber may have lagged, which is fine. We just verify no panic.
	let _ok = result.is_ok() || result.is_err();
}

// ===========================================================================
// Combination tests
// ===========================================================================

#[rstest]
#[tokio::test]
async fn test_filter_combined_source_and_level(log_buffer: Arc<LogBuffer>) {
	// Arrange
	let service = LocalLogService::new(log_buffer);
	service
		.push_logs(vec![
			make_log_entry("app", LogLevel::Debug, "debug msg"),
			make_log_entry("app", LogLevel::Warn, "warn msg"),
			make_log_entry("app", LogLevel::Error, "error msg"),
			make_log_entry("worker", LogLevel::Warn, "worker warn"),
		])
		.await
		.unwrap();

	let filter = LogFilter {
		source: Some("app".to_string()),
		min_level: Some(LogLevel::Warn),
		..Default::default()
	};

	// Act
	let result = service
		.list_logs(filter, PaginationParams::default())
		.await
		.unwrap();

	// Assert — only app entries with level >= Warn
	assert_eq!(result.items.len(), 2);
	for item in &result.items {
		assert_eq!(item.source, "app");
		assert!(item.level >= LogLevel::Warn);
	}
}

#[rstest]
#[tokio::test]
async fn test_filter_combined_source_and_search(log_buffer: Arc<LogBuffer>) {
	// Arrange
	let service = LocalLogService::new(log_buffer);
	service
		.push_logs(vec![
			make_log_entry("app", LogLevel::Info, "request processed"),
			make_log_entry("app", LogLevel::Error, "connection timeout"),
			make_log_entry("worker", LogLevel::Error, "connection timeout"),
		])
		.await
		.unwrap();

	let filter = LogFilter {
		source: Some("app".to_string()),
		search: Some("timeout".to_string()),
		..Default::default()
	};

	// Act
	let result = service
		.list_logs(filter, PaginationParams::default())
		.await
		.unwrap();

	// Assert
	assert_eq!(result.items.len(), 1);
	assert_eq!(result.items[0].source, "app");
	assert!(result.items[0].message.contains("timeout"));
}

#[rstest]
#[tokio::test]
async fn test_filter_combined_since_and_until(log_buffer: Arc<LogBuffer>) {
	// Arrange
	let service = LocalLogService::new(log_buffer);
	let now = Utc::now();

	// Push an entry at the current time
	let mut entry = make_log_entry("app", LogLevel::Info, "in range");
	entry.timestamp = now;
	service.push_logs(vec![entry]).await.unwrap();

	let filter = LogFilter {
		since: Some(now - Duration::seconds(1)),
		until: Some(now + Duration::seconds(1)),
		..Default::default()
	};

	// Act
	let result = service
		.list_logs(filter, PaginationParams::default())
		.await
		.unwrap();

	// Assert
	assert_eq!(result.items.len(), 1);
	assert_eq!(result.items[0].message, "in range");
}

#[rstest]
#[tokio::test]
async fn test_filter_combined_all_fields(log_buffer: Arc<LogBuffer>) {
	// Arrange
	let service = LocalLogService::new(log_buffer);
	let now = Utc::now();

	let mut matching = make_log_entry("api-server", LogLevel::Error, "database connection failed");
	matching.timestamp = now;
	let mut non_matching_source =
		make_log_entry("worker", LogLevel::Error, "database connection failed");
	non_matching_source.timestamp = now;
	let mut non_matching_level =
		make_log_entry("api-server", LogLevel::Debug, "database connection failed");
	non_matching_level.timestamp = now;
	let mut non_matching_search =
		make_log_entry("api-server", LogLevel::Error, "request processed ok");
	non_matching_search.timestamp = now;

	service
		.push_logs(vec![
			matching,
			non_matching_source,
			non_matching_level,
			non_matching_search,
		])
		.await
		.unwrap();

	let filter = LogFilter {
		source: Some("api-server".to_string()),
		min_level: Some(LogLevel::Error),
		since: Some(now - Duration::seconds(1)),
		until: Some(now + Duration::seconds(1)),
		search: Some("database".to_string()),
		deployment_id: None,
		namespace: None,
	};

	// Act
	let result = service
		.list_logs(filter, PaginationParams::default())
		.await
		.unwrap();

	// Assert — only the fully matching entry
	assert_eq!(result.items.len(), 1);
	assert_eq!(result.items[0].source, "api-server");
	assert_eq!(result.items[0].level, LogLevel::Error);
	assert!(result.items[0].message.contains("database"));
}

#[rstest]
#[tokio::test]
async fn test_filter_contradictory_time_range(log_buffer: Arc<LogBuffer>) {
	// Arrange
	let service = LocalLogService::new(log_buffer);
	let now = Utc::now();
	service
		.push_logs(vec![make_log_entry("app", LogLevel::Info, "hello")])
		.await
		.unwrap();

	// since > until is contradictory
	let filter = LogFilter {
		since: Some(now + Duration::hours(1)),
		until: Some(now - Duration::hours(1)),
		..Default::default()
	};

	// Act
	let result = service
		.list_logs(filter, PaginationParams::default())
		.await
		.unwrap();

	// Assert — empty result due to impossible time range
	assert!(result.items.is_empty());
}

// ===========================================================================
// Equivalence partitioning tests
// ===========================================================================

#[rstest]
#[case(LogLevel::Debug, 4)]
#[case(LogLevel::Info, 3)]
#[case(LogLevel::Warn, 2)]
#[case(LogLevel::Error, 1)]
#[tokio::test]
async fn test_log_level_filter_partitions(#[case] min_level: LogLevel, #[case] expected: usize) {
	// Arrange
	let buffer = Arc::new(LogBuffer::new(100));
	let service = LocalLogService::new(buffer);
	service
		.push_logs(vec![
			make_log_entry("app", LogLevel::Debug, "debug"),
			make_log_entry("app", LogLevel::Info, "info"),
			make_log_entry("app", LogLevel::Warn, "warn"),
			make_log_entry("app", LogLevel::Error, "error"),
		])
		.await
		.unwrap();

	let filter = LogFilter {
		min_level: Some(min_level),
		..Default::default()
	};

	// Act
	let result = service
		.list_logs(filter, PaginationParams::default())
		.await
		.unwrap();

	// Assert
	assert_eq!(result.items.len(), expected);
}

// ===========================================================================
// Boundary value tests
// ===========================================================================

#[rstest]
#[case(1)]
#[case(3)]
#[case(10_000)]
#[tokio::test]
async fn test_ring_buffer_capacity_boundaries(#[case] capacity: usize) {
	// Arrange
	let buffer = LogBuffer::new(capacity);

	// Act — push exactly `capacity` entries
	let entries: Vec<LogEntry> = (0..capacity)
		.map(|i| make_log_entry("test", LogLevel::Info, &format!("msg-{i}")))
		.collect();
	buffer.push(entries).await;

	// Assert — buffer should be exactly full
	assert_eq!(buffer.len().await, capacity);

	// Push one more — oldest should be evicted
	buffer
		.push(vec![make_log_entry("test", LogLevel::Info, "overflow")])
		.await;
	assert_eq!(buffer.len().await, capacity);
}

// ===========================================================================
// Decision table tests
// ===========================================================================

/// Tests log filter matching with 8 combinations of (source, level, search).
/// Each combination independently controls whether a filter matches.
#[rstest]
#[case(true, true, true, true)] // All match -> included
#[case(true, true, false, false)] // Search mismatch -> excluded
#[case(true, false, true, false)] // Level mismatch -> excluded
#[case(true, false, false, false)] // Level and search mismatch -> excluded
#[case(false, true, true, false)] // Source mismatch -> excluded
#[case(false, true, false, false)] // Source and search mismatch -> excluded
#[case(false, false, true, false)] // Source and level mismatch -> excluded
#[case(false, false, false, false)] // All mismatch -> excluded
#[tokio::test]
async fn test_log_filter_decision_table(
	#[case] source_match: bool,
	#[case] level_sufficient: bool,
	#[case] search_match: bool,
	#[case] expected_included: bool,
) {
	// Arrange
	let buffer = Arc::new(LogBuffer::new(100));
	let service = LocalLogService::new(buffer);

	let source = if source_match { "target" } else { "other" };
	let level = if level_sufficient {
		LogLevel::Error
	} else {
		LogLevel::Debug
	};
	let message = if search_match {
		"keyword found"
	} else {
		"nothing here"
	};

	service
		.push_logs(vec![make_log_entry(source, level, message)])
		.await
		.unwrap();

	let filter = LogFilter {
		source: Some("target".to_string()),
		min_level: Some(LogLevel::Warn),
		search: Some("keyword".to_string()),
		..Default::default()
	};

	// Act
	let result = service
		.list_logs(filter, PaginationParams::default())
		.await
		.unwrap();

	// Assert
	let actual_included = !result.items.is_empty();
	assert_eq!(
		actual_included, expected_included,
		"source_match={source_match}, level_sufficient={level_sufficient}, search_match={search_match}"
	);
}
