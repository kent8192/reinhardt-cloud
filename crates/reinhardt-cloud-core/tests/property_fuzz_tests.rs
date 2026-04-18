//! Property-based and fuzz tests using proptest.

mod fixtures;

use proptest::prelude::*;

use reinhardt_cloud_core::auth::verify_token;
use reinhardt_cloud_core::error::ApiError;
use reinhardt_cloud_core::pagination::PaginationParams;
use reinhardt_cloud_core::services::log::buffer::{LogBuffer, matches_filter};
use reinhardt_cloud_types::log::{LogEntry, LogFilter, LogLevel};

const TEST_SECRET: &[u8] = b"test-secret-key-for-jwt-signing";

// ===========================================================================
// Fuzz tests
// ===========================================================================

proptest! {
	/// Verifying arbitrary byte strings as JWT tokens should never panic.
	#[test]
	fn fuzz_verify_token_arbitrary_bytes(token in "\\PC{0,500}") {
		// Act — call verify_token with arbitrary string
		let _ = verify_token(&token, TEST_SECRET);
		// Assert — no panic occurred
	}

	/// Matching arbitrary log entries against arbitrary filters should
	/// always return a boolean without panicking.
	#[test]
	fn fuzz_matches_filter_arbitrary_inputs(
		source in "\\PC{0,50}",
		message in "\\PC{0,100}",
		level_idx in 0u8..4,
		filter_source in proptest::option::of("\\PC{0,30}"),
		filter_search in proptest::option::of("\\PC{0,30}"),
		filter_level_idx in proptest::option::of(0u8..4),
	) {
		let level = match level_idx {
			0 => LogLevel::Debug,
			1 => LogLevel::Info,
			2 => LogLevel::Warn,
			_ => LogLevel::Error,
		};
		let entry = LogEntry {
			timestamp: chrono::Utc::now(),
			level,
			source,
			message,
			metadata: None,
		};
		let filter = LogFilter {
			source: filter_source,
			min_level: filter_level_idx.map(|i| match i {
				0 => LogLevel::Debug,
				1 => LogLevel::Info,
				2 => LogLevel::Warn,
				_ => LogLevel::Error,
			}),
			since: None,
			until: None,
			search: filter_search,
			deployment_id: None,
		};

		// Act
		let result = matches_filter(&entry, &filter);

		// Assert — returns a bool without panic
		let _ = result;
	}

	/// Arbitrary pagination params should never panic and always produce
	/// valid clamped values.
	#[test]
	fn fuzz_pagination_params_arbitrary(page in 0u64..=u64::MAX, page_size in 0u64..=u64::MAX) {
		// Act
		let params = PaginationParams::new(Some(page), Some(page_size));
		let effective_page = params.page();
		let effective_size = params.page_size();

		// Assert — invariants
		prop_assert!(effective_page >= 1, "page must be >= 1, got {effective_page}");
		prop_assert!(effective_size >= 1, "page_size must be >= 1, got {effective_size}");
		prop_assert!(effective_size <= 100, "page_size must be <= 100, got {effective_size}");
	}
}

// ===========================================================================
// Property-based tests
// ===========================================================================

proptest! {
	/// offset(page+1) - offset(page) == page_size for all valid pages.
	#[test]
	fn prop_pagination_offset_monotonic(page in 1u64..10_000, page_size in 1u64..100) {
		let params_current = PaginationParams::new(Some(page), Some(page_size));
		let params_next = PaginationParams::new(Some(page + 1), Some(page_size));

		let offset_current = params_current.offset();
		let offset_next = params_next.offset();

		prop_assert_eq!(offset_next - offset_current, params_current.page_size());
	}

	/// total_pages * page_size >= total for any valid combination.
	#[test]
	fn prop_pagination_total_pages_covers_all(total in 0u64..100_000, page_size in 1u64..100) {
		let params = PaginationParams::new(Some(1), Some(page_size));
		let items: Vec<String> = Vec::new();
		let response = reinhardt_cloud_core::pagination::PaginatedResponse::new(items, total, &params);

		let effective_size = params.page_size();
		let coverage = response.total_pages.saturating_mul(effective_size);
		prop_assert!(
			coverage >= total,
			"total_pages * page_size ({coverage}) should cover total ({total})"
		);
	}

	/// A default (empty) filter matches any log entry.
	#[test]
	fn prop_log_filter_empty_matches_everything(
		source in "\\PC{1,30}",
		message in "\\PC{1,50}",
		level_idx in 0u8..4,
	) {
		let level = match level_idx {
			0 => LogLevel::Debug,
			1 => LogLevel::Info,
			2 => LogLevel::Warn,
			_ => LogLevel::Error,
		};
		let entry = LogEntry {
			timestamp: chrono::Utc::now(),
			level,
			source,
			message,
			metadata: None,
		};
		let filter = LogFilter::default();

		prop_assert!(matches_filter(&entry, &filter));
	}

	/// All ApiError variants have status codes in the 400..=599 range.
	#[test]
	fn prop_api_error_status_code_in_range(msg in "\\PC{0,50}", variant in 0u8..4) {
		let error = match variant {
			0 => ApiError::Unauthorized(msg),
			1 => ApiError::NotFound(msg),
			2 => ApiError::BadRequest(msg),
			_ => ApiError::Internal(msg),
		};

		let code = error.status_code();
		prop_assert!(
			(400..=599).contains(&code),
			"status_code {code} should be in 400..=599"
		);
	}

	/// The ring buffer should never exceed its capacity after any number of pushes.
	#[test]
	fn prop_ring_buffer_never_exceeds_capacity(capacity in 1usize..500, push_count in 0usize..1000) {
		let rt = tokio::runtime::Runtime::new().unwrap();
		rt.block_on(async {
			let buffer = LogBuffer::new(capacity);
			let entries: Vec<LogEntry> = (0..push_count)
				.map(|i| fixtures::make_log_entry("test", LogLevel::Info, &format!("msg-{i}")))
				.collect();
			buffer.push(entries).await;

			let len = buffer.len().await;
			assert!(
				len <= capacity,
				"Buffer length {len} should not exceed capacity {capacity}"
			);
		});
	}
}
