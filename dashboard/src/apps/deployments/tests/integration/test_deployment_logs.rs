//! Deployment log filtering contract.
//!
//! `deployment_logs_for_current_org` forwards `source = deployment.project_name`
//! to `LogService::list_logs`. This test exercises that filtering contract
//! against the in-memory backend (`LocalLogService`) so it does not require a
//! real Loki instance — the same contract the Loki backend honours via its
//! `{app="<source>"}` LogQL selector.

#![cfg(test)]

use std::sync::Arc;

use chrono::Utc;
use reinhardt_cloud_core::pagination::PaginationParams;
use reinhardt_cloud_core::services::log::{LocalLogService, LogBuffer};
use reinhardt_cloud_core::traits::LogService;
use reinhardt_cloud_types::log::{LogEntry, LogFilter, LogLevel};
use rstest::rstest;

#[rstest]
#[tokio::test]
async fn list_logs_filters_by_source_project_name() {
	// Arrange — one matching and one non-matching source, mirroring how
	// `deployment_logs_for_current_org` sends `source = deployment.project_name`.
	let svc = LocalLogService::new(Arc::new(LogBuffer::new(100)));
	svc.push_logs(vec![
		LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Info,
			source: "proj-a".to_string(),
			message: "match".to_string(),
			metadata: None,
		},
		LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Info,
			source: "proj-b".to_string(),
			message: "nope".to_string(),
			metadata: None,
		},
	])
	.await
	.unwrap();

	// Act — the same filter the server function builds (source = project name).
	let resp = svc
		.list_logs(
			LogFilter {
				source: Some("proj-a".to_string()),
				..Default::default()
			},
			PaginationParams::new(Some(1), Some(100)),
		)
		.await
		.unwrap();

	// Assert — only the matching project's logs are returned.
	assert_eq!(resp.items.len(), 1);
	assert_eq!(resp.items[0].source, "proj-a");
	assert_eq!(resp.items[0].message, "match");
}

#[rstest]
#[tokio::test]
async fn list_logs_min_level_filter_excludes_lower_severity() {
	// Arrange — the dashboard may narrow by minimum severity.
	let svc = LocalLogService::new(Arc::new(LogBuffer::new(100)));
	svc.push_logs(vec![
		LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Info,
			source: "proj".to_string(),
			message: "info-line".to_string(),
			metadata: None,
		},
		LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Warn,
			source: "proj".to_string(),
			message: "warn-line".to_string(),
			metadata: None,
		},
	])
	.await
	.unwrap();

	// Act
	let resp = svc
		.list_logs(
			LogFilter {
				source: Some("proj".to_string()),
				min_level: Some(LogLevel::Warn),
				..Default::default()
			},
			PaginationParams::new(Some(1), Some(100)),
		)
		.await
		.unwrap();

	// Assert — only the warn (and above) line survives the min_level filter.
	assert_eq!(resp.items.len(), 1);
	assert_eq!(resp.items[0].message, "warn-line");
}
