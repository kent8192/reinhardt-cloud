//! Loki `/loki/api/v1/tail` WebSocket consumer.
//!
//! Phase 1 ships a not-yet-implemented stub so the `list` path and unit tests
//! can land independently. The WebSocket implementation lands in Phase 2.

use std::pin::Pin;

use reinhardt_cloud_core::error::ApiError;
use reinhardt_cloud_types::log::{LogEntry, LogFilter};
use tokio_stream::Stream;

use super::LokiLogService;

/// Tail matching log entries over the Loki WebSocket.
///
/// Returns `ApiError::Internal` until the Phase 2 WebSocket implementation
/// lands. Keeping the signature stable lets `LokiLogService` satisfy the
/// `LogService` trait contract during the incremental rollout.
pub(super) async fn tail_logs(
	_svc: &LokiLogService,
	_filter: LogFilter,
) -> Result<Pin<Box<dyn Stream<Item = Result<LogEntry, ApiError>> + Send>>, ApiError> {
	Err(ApiError::Internal(
		"LokiLogService.tail_logs is not yet implemented (Phase 2)".to_string(),
	))
}
