//! Loki-backed read path implementing `reinhardt_cloud_core::traits::LogService`.
//!
//! `list_logs` calls `/loki/api/v1/query_range`; `tail_logs` opens the
//! `/loki/api/v1/tail` WebSocket. The write path is out-of-process (Promtail),
//! so `push_logs` returns `ApiError::BadRequest`.

pub(crate) mod parse;
pub(crate) mod query;
pub(crate) mod tail;

use std::time::Duration;

use async_trait::async_trait;
use reinhardt_cloud_core::error::ApiError;
use reinhardt_cloud_core::pagination::{PaginatedResponse, PaginationParams};
use reinhardt_cloud_core::traits::LogService;
use reinhardt_cloud_types::log::{LogEntry, LogFilter};

use self::parse::parse_query_range;

/// Bounded default window for `list_logs` when the filter sets no `since`.
/// Prevents unbounded `query_range` scans over all retained history.
const DEFAULT_LOOKBACK: Duration = Duration::from_secs(60 * 60); // 1 hour

/// Read-oriented Loki client implementing the core `LogService` trait.
pub struct LokiLogService {
	endpoint: String,
	http: reqwest::Client,
}

impl LokiLogService {
	/// Construct a client for the given Loki base URL (e.g.
	/// `http://loki.monitoring.svc.cluster.local:3100`).
	pub fn new(endpoint: impl Into<String>) -> Self {
		let http = reqwest::Client::builder()
			.timeout(Duration::from_secs(15))
			.build()
			.expect("valid reqwest client");
		Self {
			endpoint: endpoint.into(),
			http,
		}
	}

	/// Configured Loki base URL.
	pub fn endpoint(&self) -> &str {
		&self.endpoint
	}
}

#[async_trait]
impl LogService for LokiLogService {
	async fn push_logs(&self, _entries: Vec<LogEntry>) -> Result<(), ApiError> {
		// Write path is out-of-process (Promtail). Reject in-process pushes so a
		// caller that targets the read-oriented backend fails loudly.
		Err(ApiError::BadRequest(
			"LokiLogService is read-only; use the Promtail write path".to_string(),
		))
	}

	async fn tail_logs(
		&self,
		filter: LogFilter,
	) -> Result<std::pin::Pin<Box<dyn tokio_stream::Stream<Item = Result<LogEntry, ApiError>> + Send>>, ApiError> {
		tail::tail_logs(self, filter).await
	}

	async fn list_logs(
		&self,
		filter: LogFilter,
		pagination: PaginationParams,
	) -> Result<PaginatedResponse<LogEntry>, ApiError> {
		let logql = query::build_logql(&filter);
		// Resolve the time window. `since` defaults to a 1h lookback to bound the
		// scan; `until` defaults to now.
		let now = chrono::Utc::now();
		let start = filter
			.since
			.unwrap_or_else(|| now - chrono::Duration::from_std(DEFAULT_LOOKBACK).unwrap_or_default());
		let end = filter.until.unwrap_or(now);
		let limit = pagination.page_size();

		let mut url = reqwest::Url::parse(&format!(
			"{}/loki/api/v1/query_range",
			self.endpoint.trim_end_matches('/')
		))
		.map_err(|e| ApiError::Internal(format!("invalid loki endpoint: {e}")))?;
		url.query_pairs_mut()
			.append_pair("query", &logql)
			.append_pair("start", &start.timestamp_nanos_opt().unwrap_or(0).to_string())
			.append_pair("end", &end.timestamp_nanos_opt().unwrap_or(0).to_string())
			.append_pair("limit", &limit.to_string())
			.append_pair("direction", "backward");

		let resp = self
			.http
			.get(url)
			.send()
			.await
			.map_err(|e| ApiError::Internal(format!("loki query_range request failed: {e}")))?
			.text()
			.await
			.map_err(|e| ApiError::Internal(format!("loki query_range body failed: {e}")))?;

		let entries = parse_query_range(&resp)
			.map_err(|e| ApiError::Internal(format!("loki query_range parse failed: {e}")))?;

		// Loki returns no total count; report the page length as a best-effort
		// total and a single page (pagination is window+limit based, not offset).
		let total = entries.len() as u64;
		Ok(PaginatedResponse::new(entries, total, &pagination))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn endpoint_is_preserved() {
		// Arrange + Act
		let svc = LokiLogService::new("http://loki:3100");

		// Assert
		assert_eq!(svc.endpoint(), "http://loki:3100");
	}

	#[rstest]
	#[tokio::test]
	async fn push_logs_is_rejected() {
		// Arrange
		let svc = LokiLogService::new("http://loki:3100");

		// Act
		let result = svc.push_logs(Vec::new()).await;

		// Assert — read-only backend rejects in-process pushes.
		assert!(matches!(result, Err(ApiError::BadRequest(_))));
	}
}
