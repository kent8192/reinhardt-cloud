//! Loki-backed [`LogService`].
//!
//! # Write path
//!
//! `ingest` is intentionally a no-op: Reinhardt ships a Promtail / OTel
//! Collector DaemonSet via its Helm chart (`logging.loki.enabled`). Operator
//! pods emit structured JSON to stdout; the DaemonSet is the write path.
//!
//! # Read path
//!
//! `tail` and `list` are planned to issue `/loki/api/v1/tail` (WebSocket) and
//! `/loki/api/v1/query_range` respectively. The HTTP/LogQL mapping is
//! scheduled as a follow-up; until then they return a stub response so the
//! trait contract stays stable for downstream consumers (e.g., the dashboard
//! in Issue #371 that will target `InMemoryLogService` first).

use crate::{
	log_service::{LogFilter, LogService, LogServiceError, Pagination, RetentionPolicy},
	schema::LogRecord,
};
use async_trait::async_trait;
use futures::stream::BoxStream;
use std::time::Duration;

/// Loki-backed log service (read path is currently a stub; see module docs).
pub struct LokiLogService {
	endpoint: String,
	// Reserved for the LogQL read path that will land in a follow-up; kept on
	// the struct so construction cost (TLS handshake pool) is paid once.
	#[expect(
		dead_code,
		reason = "reserved for LogQL read path in follow-up; see module docs"
	)]
	http: reqwest::Client,
}

impl LokiLogService {
	/// Create a service pointing at the given Loki base URL (e.g.
	/// `http://loki.monitoring.svc.cluster.local:3100`).
	pub fn new(endpoint: impl Into<String>) -> Self {
		Self {
			endpoint: endpoint.into(),
			http: reqwest::Client::new(),
		}
	}

	/// Base URL of the configured Loki endpoint.
	pub fn endpoint(&self) -> &str {
		&self.endpoint
	}
}

#[async_trait]
impl LogService for LokiLogService {
	async fn ingest(&self, _record: LogRecord) -> Result<(), LogServiceError> {
		// Write path is out-of-process (Promtail / OTel Collector). Clients
		// that call `ingest` on this impl are silently accepted — no records
		// are lost because operator stdout is the canonical source.
		Ok(())
	}

	async fn tail(
		&self,
		_filter: LogFilter,
	) -> Result<BoxStream<'static, LogRecord>, LogServiceError> {
		Err(LogServiceError::Unavailable(
			"LokiLogService.tail is not yet implemented".into(),
		))
	}

	async fn list(
		&self,
		_filter: LogFilter,
		_page: Pagination,
	) -> Result<Vec<LogRecord>, LogServiceError> {
		// Empty response until the LogQL mapping lands.
		Ok(Vec::new())
	}

	fn retention_policy(&self) -> RetentionPolicy {
		// Mirrors Loki's default server-side retention (7 days).
		RetentionPolicy {
			capacity: None,
			ttl: Duration::from_secs(60 * 60 * 24 * 7),
		}
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
	fn retention_policy_reports_seven_days() {
		// Arrange + Act
		let svc = LokiLogService::new("http://loki:3100");
		let policy = svc.retention_policy();

		// Assert
		assert_eq!(policy.ttl, Duration::from_secs(60 * 60 * 24 * 7));
	}

	#[rstest]
	#[tokio::test]
	async fn ingest_is_a_no_op_and_succeeds() {
		// Arrange
		let svc = LokiLogService::new("http://loki:3100");
		let record = LogRecord::new(crate::schema::LogLevel::Info, "noop");

		// Act
		let result = svc.ingest(record).await;

		// Assert
		assert!(result.is_ok());
	}

	#[rstest]
	#[tokio::test]
	async fn tail_returns_unavailable_until_logql_lands() {
		// Arrange
		let svc = LokiLogService::new("http://loki:3100");

		// Act
		let result = svc.tail(LogFilter::default()).await;

		// Assert
		assert!(matches!(result, Err(LogServiceError::Unavailable(_))));
	}

	#[rstest]
	#[tokio::test]
	async fn list_returns_empty_until_logql_lands() {
		// Arrange
		let svc = LokiLogService::new("http://loki:3100");

		// Act
		let records = svc
			.list(LogFilter::default(), Pagination::default())
			.await
			.unwrap();

		// Assert
		assert!(records.is_empty());
	}
}
