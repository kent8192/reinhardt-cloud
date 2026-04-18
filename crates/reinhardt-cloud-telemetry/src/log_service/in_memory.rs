//! In-memory `LogService` backed by a `tokio::broadcast` channel + ring buffer.
//!
//! Honors [`RetentionPolicy`]: `capacity` caps the ring buffer, `ttl` bounds
//! record lifetime in `list` queries. Defaults: `capacity = 1000`,
//! `ttl = 1 hour`.

use crate::{
	log_service::{LogFilter, LogService, LogServiceError, Pagination, RetentionPolicy},
	schema::LogRecord,
};
use async_trait::async_trait;
use chrono::Utc;
use futures::stream::{BoxStream, StreamExt};
use std::{collections::VecDeque, sync::Mutex, time::Duration};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

/// In-process `LogService` backed by a bounded ring buffer and a
/// `tokio::broadcast` fan-out for live tail.
pub struct InMemoryLogService {
	buffer: Mutex<VecDeque<LogRecord>>,
	tx: broadcast::Sender<LogRecord>,
	policy: RetentionPolicy,
}

impl InMemoryLogService {
	/// Create a service with the given retention policy.
	pub fn new(policy: RetentionPolicy) -> Self {
		let (tx, _) = broadcast::channel(policy.capacity.max(16));
		Self {
			buffer: Mutex::new(VecDeque::with_capacity(policy.capacity)),
			tx,
			policy,
		}
	}
}

impl Default for InMemoryLogService {
	fn default() -> Self {
		Self::new(RetentionPolicy {
			capacity: 1000,
			ttl: Duration::from_secs(3600),
		})
	}
}

fn matches(record: &LogRecord, filter: &LogFilter) -> bool {
	if let Some(ref rid) = filter.reconcile_id
		&& record.fields.reconcile_id.as_ref() != Some(rid)
	{
		return false;
	}
	if let Some(ref ns) = filter.namespace
		&& record.fields.resource_namespace.as_ref() != Some(ns)
	{
		return false;
	}
	if let Some(ref did) = filter.deployment_id
		&& record.fields.deployment_id.as_ref() != Some(did)
	{
		return false;
	}
	if let Some(min) = filter.min_level
		&& (record.level as u8) < (min as u8)
	{
		return false;
	}
	true
}

#[async_trait]
impl LogService for InMemoryLogService {
	async fn ingest(&self, record: LogRecord) -> Result<(), LogServiceError> {
		let mut buf = self.buffer.lock().expect("buffer mutex poisoned");
		if buf.len() == self.policy.capacity {
			buf.pop_front();
		}
		buf.push_back(record.clone());
		// send is best-effort; no subscribers is fine.
		let _ = self.tx.send(record);
		Ok(())
	}

	/// Subscribers that lag behind the broadcast capacity will silently drop
	/// records (`BroadcastStream::Err(Lagged)` is filtered out).
	async fn tail(
		&self,
		filter: LogFilter,
	) -> Result<BoxStream<'static, LogRecord>, LogServiceError> {
		let rx = self.tx.subscribe();
		let stream = BroadcastStream::new(rx)
			.filter_map(move |r| {
				let filter = filter.clone();
				async move {
					match r {
						Ok(rec) if matches(&rec, &filter) => Some(rec),
						_ => None,
					}
				}
			})
			.boxed();
		Ok(stream)
	}

	async fn list(
		&self,
		filter: LogFilter,
		page: Pagination,
	) -> Result<Vec<LogRecord>, LogServiceError> {
		let cutoff = Utc::now()
			- chrono::Duration::from_std(self.policy.ttl)
				.expect("retention TTL must be within chrono::Duration range");
		let buf = self.buffer.lock().expect("buffer mutex poisoned");
		let filtered: Vec<_> = buf
			.iter()
			.filter(|r| r.ts >= cutoff && matches(r, &filter))
			.skip(page.offset)
			.take(page.limit)
			.cloned()
			.collect();
		Ok(filtered)
	}

	fn retention_policy(&self) -> RetentionPolicy {
		self.policy
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::schema::LogLevel;
	use rstest::rstest;

	fn rec(level: LogLevel, msg: &str) -> LogRecord {
		LogRecord::new(level, msg)
	}

	#[rstest]
	#[tokio::test]
	async fn ingest_and_list_returns_records() {
		// Arrange
		let svc = InMemoryLogService::default();

		// Act
		svc.ingest(rec(LogLevel::Info, "a")).await.unwrap();
		svc.ingest(rec(LogLevel::Info, "b")).await.unwrap();
		let listed = svc
			.list(LogFilter::default(), Pagination::default())
			.await
			.unwrap();

		// Assert
		assert_eq!(listed.len(), 2);
		assert_eq!(listed[0].msg, "a");
		assert_eq!(listed[1].msg, "b");
	}

	#[rstest]
	#[tokio::test]
	async fn capacity_bounds_the_ring_buffer() {
		// Arrange
		let svc = InMemoryLogService::new(RetentionPolicy {
			capacity: 2,
			ttl: Duration::from_secs(60),
		});

		// Act
		for i in 0..5 {
			svc.ingest(rec(LogLevel::Info, &format!("{i}")))
				.await
				.unwrap();
		}
		let listed = svc
			.list(LogFilter::default(), Pagination::default())
			.await
			.unwrap();

		// Assert: only the last 2 survived
		assert_eq!(listed.len(), 2);
		assert_eq!(listed[0].msg, "3");
		assert_eq!(listed[1].msg, "4");
	}

	#[rstest]
	#[tokio::test]
	async fn filter_by_reconcile_id_narrows_results() {
		// Arrange
		let svc = InMemoryLogService::default();
		let mut a = rec(LogLevel::Info, "a");
		a.fields.reconcile_id = Some("X".into());
		let mut b = rec(LogLevel::Info, "b");
		b.fields.reconcile_id = Some("Y".into());
		svc.ingest(a).await.unwrap();
		svc.ingest(b).await.unwrap();

		// Act
		let filter = LogFilter {
			reconcile_id: Some("X".into()),
			..Default::default()
		};
		let listed = svc.list(filter, Pagination::default()).await.unwrap();

		// Assert
		assert_eq!(listed.len(), 1);
		assert_eq!(listed[0].msg, "a");
	}

	#[rstest]
	#[tokio::test]
	async fn filter_by_deployment_id_narrows_results() {
		// Arrange
		let svc = InMemoryLogService::default();
		let mut a = rec(LogLevel::Info, "a");
		a.fields.deployment_id = Some("dep-1".into());
		let mut b = rec(LogLevel::Info, "b");
		b.fields.deployment_id = Some("dep-2".into());
		svc.ingest(a).await.unwrap();
		svc.ingest(b).await.unwrap();

		// Act
		let filter = LogFilter {
			deployment_id: Some("dep-1".into()),
			..Default::default()
		};
		let listed = svc.list(filter, Pagination::default()).await.unwrap();

		// Assert
		assert_eq!(listed.len(), 1);
		assert_eq!(listed[0].msg, "a");
	}

	#[rstest]
	#[tokio::test]
	async fn tail_delivers_live_records_to_subscribers() {
		// Arrange
		let svc = InMemoryLogService::default();
		let mut stream = svc.tail(LogFilter::default()).await.unwrap();

		// Act
		svc.ingest(rec(LogLevel::Warn, "live")).await.unwrap();
		let delivered = tokio::time::timeout(Duration::from_millis(100), stream.next())
			.await
			.expect("timed out")
			.expect("stream closed");

		// Assert
		assert_eq!(delivered.msg, "live");
		assert_eq!(delivered.level, LogLevel::Warn);
	}
}
