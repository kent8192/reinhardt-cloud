//! Local in-process log service implementation.

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use tokio_stream::Stream;

use crate::error::ApiError;
use crate::pagination::{PaginatedResponse, PaginationParams};
use crate::services::log::buffer::{LogBuffer, matches_filter};
use crate::traits::LogService;
use reinhardt_cloud_types::log::{LogEntry, LogFilter};

/// Local log service backed by an in-memory ring buffer.
pub struct LocalLogService {
	buffer: Arc<LogBuffer>,
}

impl LocalLogService {
	pub fn new(buffer: Arc<LogBuffer>) -> Self {
		Self { buffer }
	}
}

#[async_trait]
impl LogService for LocalLogService {
	async fn push_logs(&self, entries: Vec<LogEntry>) -> Result<(), ApiError> {
		self.buffer.push(entries).await;
		Ok(())
	}

	async fn tail_logs(
		&self,
		filter: LogFilter,
	) -> Result<Pin<Box<dyn Stream<Item = Result<LogEntry, ApiError>> + Send>>, ApiError> {
		let mut rx = self.buffer.subscribe();

		let stream = async_stream::stream! {
			loop {
				match rx.recv().await {
					Ok(entry) => {
						if matches_filter(&entry, &filter) {
							yield Ok(entry);
						}
					}
					Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
						tracing::warn!("Log tail subscriber lagged by {n} entries");
					}
					Err(tokio::sync::broadcast::error::RecvError::Closed) => {
						break;
					}
				}
			}
		};

		Ok(Box::pin(stream))
	}

	async fn list_logs(
		&self,
		filter: LogFilter,
		pagination: PaginationParams,
	) -> Result<PaginatedResponse<LogEntry>, ApiError> {
		let all = self.buffer.query(&filter).await;
		let total = all.len() as u64;
		let offset = pagination.offset() as usize;
		let page_size = pagination.page_size() as usize;

		let items: Vec<LogEntry> = all.into_iter().skip(offset).take(page_size).collect();

		Ok(PaginatedResponse::new(items, total, &pagination))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use chrono::Utc;
	use reinhardt_cloud_types::log::LogLevel;
	use rstest::rstest;

	fn make_entry(msg: &str) -> LogEntry {
		LogEntry {
			timestamp: Utc::now(),
			level: LogLevel::Info,
			source: "test".to_string(),
			message: msg.to_string(),
			metadata: None,
		}
	}

	#[rstest]
	#[tokio::test]
	async fn test_push_and_list() {
		// Arrange
		let buffer = Arc::new(LogBuffer::new(100));
		let service = LocalLogService::new(buffer);

		// Act
		service
			.push_logs(vec![make_entry("a"), make_entry("b"), make_entry("c")])
			.await
			.unwrap();

		let result = service
			.list_logs(LogFilter::default(), PaginationParams::default())
			.await
			.unwrap();

		// Assert
		assert_eq!(result.total, 3);
		assert_eq!(result.items.len(), 3);
	}

	#[rstest]
	#[tokio::test]
	async fn test_list_with_pagination() {
		// Arrange
		let buffer = Arc::new(LogBuffer::new(100));
		let service = LocalLogService::new(buffer);

		let entries: Vec<LogEntry> = (0..10).map(|i| make_entry(&format!("msg-{i}"))).collect();
		service.push_logs(entries).await.unwrap();

		// Act — page 2, page_size 3
		let result = service
			.list_logs(
				LogFilter::default(),
				PaginationParams::new(Some(2), Some(3)),
			)
			.await
			.unwrap();

		// Assert
		assert_eq!(result.total, 10);
		assert_eq!(result.items.len(), 3);
		assert_eq!(result.page, 2);
	}
}
