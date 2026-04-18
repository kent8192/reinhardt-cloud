//! [`LogService`] trait and shared types.
//!
//! See `in_memory` for the default dev backend and `loki` for the
//! Loki-backed backend.

pub(crate) mod in_memory;
pub(crate) mod loki;

use crate::schema::{LogLevel, LogRecord};
use async_trait::async_trait;
use futures::stream::BoxStream;
use std::time::Duration;
use thiserror::Error;

/// Filter criteria applied by [`LogService::tail`] and [`LogService::list`].
///
/// All fields are additive — when set, only records matching every
/// populated criterion are returned.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogFilter {
	pub reconcile_id: Option<String>,
	pub deployment_id: Option<String>,
	pub namespace: Option<String>,
	pub min_level: Option<LogLevel>,
}

/// Pagination window for [`LogService::list`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pagination {
	pub offset: usize,
	pub limit: usize,
}

impl Default for Pagination {
	fn default() -> Self {
		Self {
			offset: 0,
			limit: 100,
		}
	}
}

/// Declared capacity and TTL of a [`LogService`] backend.
///
/// Surfaced to consumers so the dashboard's ring-buffer contract (see #371)
/// stays consistent with the backend's actual retention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionPolicy {
	pub capacity: usize,
	pub ttl: Duration,
}

/// Errors returned by [`LogService`] implementations.
#[derive(Debug, Error)]
pub enum LogServiceError {
	/// The backend rejected the record (validation failure, quota, etc.).
	#[error("backend rejected the record: {0}")]
	Rejected(String),
	/// The backend is currently unreachable.
	#[error("backend unavailable: {0}")]
	Unavailable(String),
	/// JSON (de)serialization failed while processing a record.
	#[error("serialization error: {0}")]
	Serialization(#[from] serde_json::Error),
}

/// Abstract interface over a log backend.
///
/// Implementations must be `Send + Sync` so they can be shared across
/// reconcile tasks. See `in_memory::InMemoryLogService` for the default
/// dev backend.
#[async_trait]
pub trait LogService: Send + Sync {
	/// Append a record. In-process impls buffer; external impls may no-op.
	async fn ingest(&self, record: LogRecord) -> Result<(), LogServiceError>;

	/// Live-tail matching records as they arrive.
	async fn tail(
		&self,
		filter: LogFilter,
	) -> Result<BoxStream<'static, LogRecord>, LogServiceError>;

	/// Historical query of matching records.
	async fn list(
		&self,
		filter: LogFilter,
		page: Pagination,
	) -> Result<Vec<LogRecord>, LogServiceError>;

	/// Capacity and TTL the backend declares to consumers.
	fn retention_policy(&self) -> RetentionPolicy;
}
