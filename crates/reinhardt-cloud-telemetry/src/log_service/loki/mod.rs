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
use futures::StreamExt;
use reinhardt_cloud_core::error::ApiError;
use reinhardt_cloud_core::pagination::{PaginatedResponse, PaginationParams};
use reinhardt_cloud_core::traits::LogService as CoreLogService;
use reinhardt_cloud_types::log::{LogEntry, LogFilter, LogLevel as CoreLogLevel};

use self::parse::{loki_error_message, parse_query_range};
use crate::log_service::{
	LogFilter as LegacyLogFilter, LogService as LegacyLogService, LogServiceError, Pagination,
	RetentionPolicy,
};
use crate::schema::{LogFields, LogLevel, LogRecord};

/// Bounded default window for `list_logs` when the filter sets no `since`.
/// Prevents unbounded `query_range` scans over all retained history.
const DEFAULT_LOOKBACK: Duration = Duration::from_secs(60 * 60); // 1 hour

/// Maximum rows fetched while bridging legacy post-filtered pagination.
const LEGACY_PREFILTER_SCAN_LIMIT: usize = 5_000;

fn timestamp_nanos(timestamp: chrono::DateTime<chrono::Utc>, field: &str) -> Result<i64, ApiError> {
	timestamp
		.timestamp_nanos_opt()
		.ok_or_else(|| ApiError::BadRequest(format!("invalid `{field}` timestamp range")))
}

fn legacy_fetch_target(page: Pagination) -> usize {
	page.offset.saturating_add(page.limit).max(1)
}

fn next_legacy_prefilter_limit(current: usize, target: usize) -> usize {
	let cap = target.max(LEGACY_PREFILTER_SCAN_LIMIT);
	current.saturating_mul(2).max(target).min(cap)
}

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

	async fn query_range_entries(
		&self,
		filter: &LogFilter,
		limit: u64,
	) -> Result<Vec<LogEntry>, ApiError> {
		let logql = query::build_logql(filter);
		// Resolve the time window. `since` defaults to a 1h lookback to bound the
		// scan; `until` defaults to now.
		let now = chrono::Utc::now();
		let start = filter.since.unwrap_or_else(|| {
			now - chrono::Duration::from_std(DEFAULT_LOOKBACK).unwrap_or_default()
		});
		let end = filter.until.unwrap_or(now);
		let start_ns = timestamp_nanos(start, "since")?;
		let end_ns = timestamp_nanos(end, "until")?;

		let mut url = reqwest::Url::parse(&format!(
			"{}/loki/api/v1/query_range",
			self.endpoint.trim_end_matches('/')
		))
		.map_err(|e| ApiError::Internal(format!("invalid loki endpoint: {e}")))?;
		url.query_pairs_mut()
			.append_pair("query", &logql)
			.append_pair("start", &start_ns.to_string())
			.append_pair("end", &end_ns.to_string())
			.append_pair("limit", &limit.max(1).to_string())
			.append_pair("direction", "backward");

		let response = self
			.http
			.get(url)
			.send()
			.await
			.map_err(|e| ApiError::Internal(format!("loki query_range request failed: {e}")))?;
		let status = response.status();
		let resp = response
			.text()
			.await
			.map_err(|e| ApiError::Internal(format!("loki query_range body failed: {e}")))?;
		if !status.is_success() {
			return Err(ApiError::Internal(format!(
				"loki query_range returned HTTP {status}: {resp}"
			)));
		}
		if let Some(message) = loki_error_message(&resp) {
			return Err(ApiError::Internal(format!(
				"loki query_range returned error: {message}"
			)));
		}

		parse_query_range(&resp)
			.map_err(|e| ApiError::Internal(format!("loki query_range parse failed: {e}")))
	}
}

#[async_trait]
impl CoreLogService for LokiLogService {
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
	) -> Result<
		std::pin::Pin<Box<dyn tokio_stream::Stream<Item = Result<LogEntry, ApiError>> + Send>>,
		ApiError,
	> {
		tail::tail_logs(self, filter).await
	}

	async fn list_logs(
		&self,
		filter: LogFilter,
		pagination: PaginationParams,
	) -> Result<PaginatedResponse<LogEntry>, ApiError> {
		let offset = pagination.offset() as usize;
		let page_size = pagination.page_size() as usize;
		let fetch_limit = offset.saturating_add(page_size).max(1) as u64;
		let entries = self.query_range_entries(&filter, fetch_limit).await?;
		let total = entries.len() as u64;
		let items = entries.into_iter().skip(offset).take(page_size).collect();

		// Loki returns no total count; report the fetched window length as a
		// best-effort total for the bounded query window.
		Ok(PaginatedResponse::new(items, total, &pagination))
	}
}

fn api_error_to_legacy(error: ApiError) -> LogServiceError {
	match error {
		ApiError::BadRequest(message) => LogServiceError::Rejected(message),
		other => LogServiceError::Unavailable(other.to_string()),
	}
}

fn legacy_level_to_core(level: LogLevel) -> CoreLogLevel {
	match level {
		LogLevel::Trace | LogLevel::Debug => CoreLogLevel::Debug,
		LogLevel::Info => CoreLogLevel::Info,
		LogLevel::Warn => CoreLogLevel::Warn,
		LogLevel::Error => CoreLogLevel::Error,
	}
}

fn core_level_to_legacy(level: CoreLogLevel) -> LogLevel {
	match level {
		CoreLogLevel::Debug => LogLevel::Debug,
		CoreLogLevel::Info => LogLevel::Info,
		CoreLogLevel::Warn => LogLevel::Warn,
		CoreLogLevel::Error => LogLevel::Error,
	}
}

fn legacy_filter_to_core(filter: LegacyLogFilter) -> LogFilter {
	LogFilter {
		min_level: filter.min_level.map(legacy_level_to_core),
		deployment_id: filter.deployment_id,
		..Default::default()
	}
}

fn legacy_filter_requires_post_filter(filter: &LegacyLogFilter) -> bool {
	filter.reconcile_id.is_some() || filter.namespace.is_some()
}

fn legacy_record_matches_post_filter(record: &LogRecord, filter: &LegacyLogFilter) -> bool {
	if let Some(ref reconcile_id) = filter.reconcile_id
		&& record.fields.reconcile_id.as_ref() != Some(reconcile_id)
	{
		return false;
	}
	if let Some(ref namespace) = filter.namespace
		&& record.fields.resource_namespace.as_ref() != Some(namespace)
	{
		return false;
	}
	true
}

fn core_entry_to_legacy(entry: LogEntry) -> LogRecord {
	let fields = entry
		.metadata
		.and_then(|value| serde_json::from_value::<LogFields>(value).ok())
		.unwrap_or_default();
	LogRecord {
		ts: entry.timestamp,
		level: core_level_to_legacy(entry.level),
		msg: entry.message,
		fields,
	}
}

#[async_trait]
impl LegacyLogService for LokiLogService {
	async fn ingest(&self, _record: LogRecord) -> Result<(), LogServiceError> {
		Err(LogServiceError::Rejected(
			"LokiLogService is read-only; use the Promtail write path".to_string(),
		))
	}

	async fn tail(
		&self,
		filter: LegacyLogFilter,
	) -> Result<futures::stream::BoxStream<'static, LogRecord>, LogServiceError> {
		let post_filter = filter.clone();
		let stream = CoreLogService::tail_logs(self, legacy_filter_to_core(filter))
			.await
			.map_err(api_error_to_legacy)?;
		Ok(stream
			.filter_map(move |entry| {
				let post_filter = post_filter.clone();
				async move {
					entry
						.ok()
						.map(core_entry_to_legacy)
						.filter(|record| legacy_record_matches_post_filter(record, &post_filter))
				}
			})
			.boxed())
	}

	async fn list(
		&self,
		filter: LegacyLogFilter,
		page: Pagination,
	) -> Result<Vec<LogRecord>, LogServiceError> {
		let target = legacy_fetch_target(page);
		let core_filter = legacy_filter_to_core(filter.clone());
		if !legacy_filter_requires_post_filter(&filter) {
			let entries = self
				.query_range_entries(&core_filter, target as u64)
				.await
				.map_err(api_error_to_legacy)?;
			return Ok(entries
				.into_iter()
				.map(core_entry_to_legacy)
				.skip(page.offset)
				.take(page.limit)
				.collect());
		}

		let scan_cap = target.max(LEGACY_PREFILTER_SCAN_LIMIT);
		let mut fetch_limit = target;
		loop {
			let entries = self
				.query_range_entries(&core_filter, fetch_limit as u64)
				.await
				.map_err(api_error_to_legacy)?;
			let raw_count = entries.len();
			let records = entries
				.into_iter()
				.map(core_entry_to_legacy)
				.filter(|record| legacy_record_matches_post_filter(record, &filter))
				.collect::<Vec<_>>();
			if records.len() >= target || raw_count < fetch_limit || fetch_limit >= scan_cap {
				return Ok(records
					.into_iter()
					.skip(page.offset)
					.take(page.limit)
					.collect());
			}
			let next_limit = next_legacy_prefilter_limit(fetch_limit, target);
			if next_limit == fetch_limit {
				return Ok(records
					.into_iter()
					.skip(page.offset)
					.take(page.limit)
					.collect());
			}
			fetch_limit = next_limit;
		}
	}

	fn retention_policy(&self) -> RetentionPolicy {
		RetentionPolicy {
			capacity: None,
			ttl: Duration::from_secs(60 * 60 * 24 * 7),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use chrono::TimeZone;
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

	#[rstest]
	#[tokio::test]
	async fn legacy_ingest_is_rejected() {
		// Arrange
		let svc = LokiLogService::new("http://loki:3100");

		// Act
		let result =
			LegacyLogService::ingest(&svc, LogRecord::new(LogLevel::Info, "message")).await;

		// Assert
		match result {
			Err(LogServiceError::Rejected(message)) => {
				assert_eq!(
					message,
					"LokiLogService is read-only; use the Promtail write path"
				);
			}
			_ => panic!("expected read-only legacy ingest rejection"),
		}
	}

	#[rstest]
	fn timestamp_nanos_rejects_out_of_range_dates() {
		// Arrange
		let timestamp = chrono::Utc.with_ymd_and_hms(3000, 1, 1, 0, 0, 0).unwrap();

		// Act
		let result = timestamp_nanos(timestamp, "since");

		// Assert
		match result {
			Err(ApiError::BadRequest(message)) => {
				assert_eq!(message, "invalid `since` timestamp range");
			}
			_ => panic!("expected bad request for out-of-range timestamp"),
		}
	}

	#[rstest]
	fn legacy_prefilter_limit_grows_until_scan_cap() {
		// Arrange
		let target = 125;

		// Act + Assert
		assert_eq!(next_legacy_prefilter_limit(125, target), 250);
		assert_eq!(
			next_legacy_prefilter_limit(LEGACY_PREFILTER_SCAN_LIMIT, target),
			LEGACY_PREFILTER_SCAN_LIMIT
		);
	}

	async fn loki_endpoint_with_response(
		status_line: &str,
		body: &str,
	) -> (String, tokio::task::JoinHandle<()>) {
		use tokio::io::{AsyncReadExt, AsyncWriteExt};

		let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
			.await
			.expect("bind test listener");
		let addr = listener.local_addr().expect("listener addr");
		let status_line = status_line.to_string();
		let body = body.to_string();
		let handle = tokio::spawn(async move {
			let (mut socket, _) = listener.accept().await.expect("accept request");
			let mut buf = [0u8; 1024];
			let _ = socket.read(&mut buf).await;
			let response = format!(
				"{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
				body.len()
			);
			socket
				.write_all(response.as_bytes())
				.await
				.expect("write response");
		});
		(format!("http://{addr}"), handle)
	}

	#[rstest]
	#[tokio::test]
	async fn list_logs_surfaces_loki_http_error_status() {
		// Arrange
		let (endpoint, server) =
			loki_endpoint_with_response("HTTP/1.1 500 Internal Server Error", "boom").await;
		let svc = LokiLogService::new(endpoint);

		// Act
		let result = svc
			.list_logs(LogFilter::default(), PaginationParams::default())
			.await;
		server.await.expect("server task");

		// Assert
		match result {
			Err(ApiError::Internal(message)) => {
				assert_eq!(
					message,
					"loki query_range returned HTTP 500 Internal Server Error: boom"
				);
			}
			_ => panic!("expected HTTP status ApiError"),
		}
	}

	#[rstest]
	#[tokio::test]
	async fn list_logs_surfaces_loki_status_error_body() {
		// Arrange
		let (endpoint, server) = loki_endpoint_with_response(
			"HTTP/1.1 200 OK",
			r#"{"status":"error","error":"bad logql"}"#,
		)
		.await;
		let svc = LokiLogService::new(endpoint);

		// Act
		let result = svc
			.list_logs(LogFilter::default(), PaginationParams::default())
			.await;
		server.await.expect("server task");

		// Assert
		match result {
			Err(ApiError::Internal(message)) => {
				assert_eq!(message, "loki query_range returned error: bad logql");
			}
			_ => panic!("expected Loki status error ApiError"),
		}
	}

	#[rstest]
	fn legacy_post_filter_matches_reconcile_id_and_namespace() {
		// Arrange
		let mut record = LogRecord::new(LogLevel::Info, "message");
		record.fields.reconcile_id = Some("r-1".to_string());
		record.fields.resource_namespace = Some("tenant-a".to_string());
		let matching = LegacyLogFilter {
			reconcile_id: Some("r-1".to_string()),
			namespace: Some("tenant-a".to_string()),
			..Default::default()
		};
		let mismatched = LegacyLogFilter {
			reconcile_id: Some("r-2".to_string()),
			namespace: Some("tenant-a".to_string()),
			..Default::default()
		};

		// Act + Assert
		assert_eq!(legacy_record_matches_post_filter(&record, &matching), true);
		assert_eq!(
			legacy_record_matches_post_filter(&record, &mismatched),
			false
		);
	}

	#[rstest]
	fn implements_legacy_log_service_trait() {
		// Arrange + Act
		fn assert_legacy_impl<T: crate::LogService>() {}

		// Assert
		assert_legacy_impl::<LokiLogService>();
	}

	#[rstest]
	fn legacy_retention_policy_reports_loki_default() {
		// Arrange
		let svc = LokiLogService::new("http://loki:3100");

		// Act
		let policy = LegacyLogService::retention_policy(&svc);

		// Assert
		assert_eq!(policy.capacity, None);
		assert_eq!(policy.ttl, Duration::from_secs(60 * 60 * 24 * 7));
	}
}
