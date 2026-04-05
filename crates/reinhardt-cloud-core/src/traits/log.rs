//! Log service trait.

use std::pin::Pin;

use async_trait::async_trait;
use tokio_stream::Stream;

use crate::error::ApiError;
use crate::pagination::{PaginatedResponse, PaginationParams};
use reinhardt_cloud_types::log::{LogEntry, LogFilter};

/// Trait for log ingestion, querying, and real-time tailing.
///
/// Implementations handle storing log entries, querying with filters
/// and pagination, and streaming logs in real-time.
#[async_trait]
pub trait LogService: Send + Sync + 'static {
	/// Push a batch of log entries for storage.
	async fn push_logs(&self, entries: Vec<LogEntry>) -> Result<(), ApiError>;

	/// Tail logs matching the given filter in real-time.
	///
	/// Returns a stream that emits new log entries as they arrive.
	async fn tail_logs(
		&self,
		filter: LogFilter,
	) -> Result<Pin<Box<dyn Stream<Item = Result<LogEntry, ApiError>> + Send>>, ApiError>;

	/// List stored logs matching the filter with pagination.
	async fn list_logs(
		&self,
		filter: LogFilter,
		pagination: PaginationParams,
	) -> Result<PaginatedResponse<LogEntry>, ApiError>;
}
