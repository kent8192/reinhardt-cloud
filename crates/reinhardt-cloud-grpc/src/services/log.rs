//! gRPC Log Service server and client implementations.

use std::pin::Pin;
use std::sync::Arc;

use prost_types::Timestamp;
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status, Streaming};

use reinhardt_cloud_core::ApiError;
use reinhardt_cloud_core::pagination::PaginationParams;
use reinhardt_cloud_core::traits::LogService;
use reinhardt_cloud_proto::common::{PaginationResponse, StatusResponse};
use reinhardt_cloud_proto::log as pb;
use reinhardt_cloud_types::log::{LogEntry, LogFilter, LogLevel};

// --- Error mapping ---

fn api_error_to_status(e: ApiError) -> Status {
	match e {
		ApiError::NotFound(msg) => Status::not_found(msg),
		ApiError::BadRequest(msg) => Status::invalid_argument(msg),
		ApiError::Unauthorized(msg) => Status::unauthenticated(msg),
		ApiError::Internal(msg) => Status::internal(msg),
	}
}

// --- Conversions ---

fn timestamp_from_chrono(dt: chrono::DateTime<chrono::Utc>) -> Option<Timestamp> {
	Some(Timestamp {
		seconds: dt.timestamp(),
		nanos: dt.timestamp_subsec_nanos() as i32,
	})
}

fn proto_timestamp_to_chrono(
	ts: Option<Timestamp>,
) -> Result<chrono::DateTime<chrono::Utc>, Status> {
	let t = ts.ok_or_else(|| Status::invalid_argument("missing required timestamp"))?;
	chrono::DateTime::from_timestamp(t.seconds, t.nanos.try_into().unwrap_or(0)).ok_or_else(|| {
		Status::invalid_argument(format!(
			"invalid timestamp: seconds={}, nanos={}",
			t.seconds, t.nanos
		))
	})
}

fn proto_level_to_domain(level: i32) -> LogLevel {
	match pb::LogLevel::try_from(level) {
		Ok(pb::LogLevel::Debug) => LogLevel::Debug,
		Ok(pb::LogLevel::Info) => LogLevel::Info,
		Ok(pb::LogLevel::Warn) => LogLevel::Warn,
		Ok(pb::LogLevel::Error) => LogLevel::Error,
		_ => LogLevel::Info,
	}
}

fn domain_level_to_proto(level: &LogLevel) -> i32 {
	match level {
		LogLevel::Debug => pb::LogLevel::Debug as i32,
		LogLevel::Info => pb::LogLevel::Info as i32,
		LogLevel::Warn => pb::LogLevel::Warn as i32,
		LogLevel::Error => pb::LogLevel::Error as i32,
	}
}

fn proto_entry_to_domain(entry: &pb::LogEntry) -> Result<LogEntry, Status> {
	Ok(LogEntry {
		timestamp: proto_timestamp_to_chrono(entry.timestamp)?,
		level: proto_level_to_domain(entry.level),
		source: entry.source.clone(),
		message: entry.message.clone(),
		metadata: entry
			.metadata_json
			.as_ref()
			.and_then(|s| serde_json::from_str(s).ok()),
	})
}

fn domain_entry_to_proto(entry: &LogEntry) -> pb::LogEntry {
	pb::LogEntry {
		timestamp: timestamp_from_chrono(entry.timestamp),
		level: domain_level_to_proto(&entry.level),
		source: entry.source.clone(),
		message: entry.message.clone(),
		metadata_json: entry.metadata.as_ref().map(|m| m.to_string()),
	}
}

fn proto_filter_to_domain(filter: &Option<pb::LogFilter>) -> Result<LogFilter, Status> {
	match filter {
		Some(f) => Ok(LogFilter {
			source: f.source.clone(),
			min_level: f.min_level.map(proto_level_to_domain),
			since: f
				.since
				.map(|t| proto_timestamp_to_chrono(Some(t)))
				.transpose()?,
			until: f
				.until
				.map(|t| proto_timestamp_to_chrono(Some(t)))
				.transpose()?,
			search: f.search.clone(),
			deployment_id: f.deployment_id.clone(),
		}),
		None => Ok(LogFilter::default()),
	}
}

// --- gRPC Server ---

/// gRPC server implementation wrapping a `LogService` trait object.
pub struct LogServiceGrpc {
	service: Arc<dyn LogService>,
}

impl LogServiceGrpc {
	pub fn new(service: Arc<dyn LogService>) -> Self {
		Self { service }
	}
}

#[tonic::async_trait]
impl pb::log_service_server::LogService for LogServiceGrpc {
	type TailLogsStream =
		Pin<Box<dyn Stream<Item = Result<pb::LogEntry, Status>> + Send + 'static>>;

	async fn push_logs(
		&self,
		request: Request<Streaming<pb::PushLogsRequest>>,
	) -> Result<Response<StatusResponse>, Status> {
		let mut stream = request.into_inner();
		let mut total = 0u64;

		while let Some(result) = stream.next().await {
			let push_req = result.map_err(|e| Status::internal(e.to_string()))?;
			let entries: Vec<LogEntry> = push_req
				.entries
				.iter()
				.map(proto_entry_to_domain)
				.collect::<Result<Vec<_>, _>>()?;
			total += entries.len() as u64;
			self.service
				.push_logs(entries)
				.await
				.map_err(api_error_to_status)?;
		}

		Ok(Response::new(StatusResponse {
			success: true,
			message: format!("Received {total} log entries"),
		}))
	}

	async fn tail_logs(
		&self,
		request: Request<pb::TailLogsRequest>,
	) -> Result<Response<Self::TailLogsStream>, Status> {
		let filter = proto_filter_to_domain(&request.into_inner().filter)?;

		let stream = self
			.service
			.tail_logs(filter)
			.await
			.map_err(api_error_to_status)?;

		let mapped = stream.map(|result| match result {
			Ok(entry) => Ok(domain_entry_to_proto(&entry)),
			Err(e) => Err(api_error_to_status(e)),
		});

		Ok(Response::new(Box::pin(mapped)))
	}

	async fn list_logs(
		&self,
		request: Request<pb::ListLogsRequest>,
	) -> Result<Response<pb::ListLogsResponse>, Status> {
		let req = request.into_inner();
		let filter = proto_filter_to_domain(&req.filter)?;
		let pagination = req
			.pagination
			.map(|p| PaginationParams::new(Some(p.page), Some(p.page_size)))
			.unwrap_or_default();

		let result = self
			.service
			.list_logs(filter, pagination)
			.await
			.map_err(api_error_to_status)?;

		Ok(Response::new(pb::ListLogsResponse {
			entries: result.items.iter().map(domain_entry_to_proto).collect(),
			pagination: Some(PaginationResponse {
				total: result.total,
				page: result.page,
				page_size: result.page_size,
				total_pages: result.total_pages,
			}),
		}))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use chrono::{TimeZone, Utc};
	use prost_types::Timestamp;
	use rstest::rstest;

	/// Helper: create a fixed timestamp for deterministic testing.
	fn fixed_timestamp() -> chrono::DateTime<chrono::Utc> {
		Utc.with_ymd_and_hms(2025, 6, 15, 12, 30, 45).unwrap()
	}

	/// Helper: create a prost Timestamp from a chrono DateTime.
	fn to_proto_ts(dt: chrono::DateTime<chrono::Utc>) -> Timestamp {
		Timestamp {
			seconds: dt.timestamp(),
			nanos: dt.timestamp_subsec_nanos() as i32,
		}
	}

	// --- All 4 log levels: domain -> proto -> domain roundtrip ---

	#[rstest]
	#[case(LogLevel::Debug, pb::LogLevel::Debug as i32)]
	#[case(LogLevel::Info, pb::LogLevel::Info as i32)]
	#[case(LogLevel::Warn, pb::LogLevel::Warn as i32)]
	#[case(LogLevel::Error, pb::LogLevel::Error as i32)]
	fn test_domain_level_to_proto_all_variants(#[case] domain: LogLevel, #[case] expected: i32) {
		// Arrange — provided by #[case]

		// Act
		let proto_val = domain_level_to_proto(&domain);

		// Assert
		assert_eq!(proto_val, expected);
	}

	#[rstest]
	#[case(pb::LogLevel::Debug as i32, LogLevel::Debug)]
	#[case(pb::LogLevel::Info as i32, LogLevel::Info)]
	#[case(pb::LogLevel::Warn as i32, LogLevel::Warn)]
	#[case(pb::LogLevel::Error as i32, LogLevel::Error)]
	fn test_proto_level_to_domain_all_variants(#[case] proto_val: i32, #[case] expected: LogLevel) {
		// Arrange — provided by #[case]

		// Act
		let domain = proto_level_to_domain(proto_val);

		// Assert
		assert_eq!(domain, expected);
	}

	// --- Level roundtrip both directions ---

	#[rstest]
	#[case(LogLevel::Debug)]
	#[case(LogLevel::Info)]
	#[case(LogLevel::Warn)]
	#[case(LogLevel::Error)]
	fn test_level_roundtrip(#[case] level: LogLevel) {
		// Arrange — provided by #[case]

		// Act
		let proto = domain_level_to_proto(&level);
		let roundtripped = proto_level_to_domain(proto);

		// Assert
		assert_eq!(roundtripped, level);
	}

	// --- proto_level boundary values ---

	#[rstest]
	#[case(-1, LogLevel::Info)] // negative -> Info fallback
	#[case(0, LogLevel::Info)] // UNSPECIFIED -> Info fallback
	#[case(5, LogLevel::Info)] // out of range -> Info fallback
	#[case(99, LogLevel::Info)]
	#[case(i32::MAX, LogLevel::Info)]
	#[case(i32::MIN, LogLevel::Info)]
	fn test_proto_level_boundary_values(#[case] proto_val: i32, #[case] expected: LogLevel) {
		// Arrange — provided by #[case]

		// Act
		let domain = proto_level_to_domain(proto_val);

		// Assert
		assert_eq!(domain, expected);
	}

	// --- Log entry full roundtrip (with metadata_json) ---

	#[rstest]
	fn test_log_entry_roundtrip_with_metadata() {
		// Arrange
		let ts = fixed_timestamp();
		let entry = LogEntry {
			timestamp: ts,
			level: LogLevel::Warn,
			source: "api-gateway".to_string(),
			message: "rate limit exceeded".to_string(),
			metadata: Some(serde_json::json!({"client_ip": "10.0.0.1", "path": "/api/users"})),
		};

		// Act
		let proto = domain_entry_to_proto(&entry);
		let roundtripped = proto_entry_to_domain(&proto).unwrap();

		// Assert
		assert_eq!(roundtripped.timestamp, ts);
		assert_eq!(roundtripped.level, LogLevel::Warn);
		assert_eq!(roundtripped.source, "api-gateway");
		assert_eq!(roundtripped.message, "rate limit exceeded");
		let meta = roundtripped.metadata.unwrap();
		assert_eq!(meta["client_ip"], "10.0.0.1");
		assert_eq!(meta["path"], "/api/users");
	}

	// --- Log entry without metadata roundtrip ---

	#[rstest]
	fn test_log_entry_roundtrip_without_metadata() {
		// Arrange
		let ts = fixed_timestamp();
		let entry = LogEntry {
			timestamp: ts,
			level: LogLevel::Error,
			source: "system".to_string(),
			message: "disk full".to_string(),
			metadata: None,
		};

		// Act
		let proto = domain_entry_to_proto(&entry);
		let roundtripped = proto_entry_to_domain(&proto).unwrap();

		// Assert
		assert_eq!(roundtripped.timestamp, ts);
		assert_eq!(roundtripped.level, LogLevel::Error);
		assert_eq!(roundtripped.source, "system");
		assert_eq!(roundtripped.message, "disk full");
		assert!(roundtripped.metadata.is_none());
	}

	// --- Filter with all fields set ---

	#[rstest]
	fn test_filter_with_all_fields() {
		// Arrange
		let since = fixed_timestamp();
		let until = Utc.with_ymd_and_hms(2025, 12, 31, 23, 59, 59).unwrap();
		let proto_filter = Some(pb::LogFilter {
			source: Some("web".to_string()),
			min_level: Some(pb::LogLevel::Warn as i32),
			since: Some(to_proto_ts(since)),
			until: Some(to_proto_ts(until)),
			search: Some("error".to_string()),
			deployment_id: Some("deploy-abc".to_string()),
		});

		// Act
		let domain = proto_filter_to_domain(&proto_filter).unwrap();

		// Assert
		assert_eq!(domain.source, Some("web".to_string()));
		assert_eq!(domain.min_level, Some(LogLevel::Warn));
		assert_eq!(domain.since, Some(since));
		assert_eq!(domain.until, Some(until));
		assert_eq!(domain.search, Some("error".to_string()));
		assert_eq!(domain.deployment_id, Some("deploy-abc".to_string()));
	}

	// --- Filter with None -> default ---

	#[rstest]
	fn test_filter_none_gives_default() {
		// Arrange
		let proto_filter: Option<pb::LogFilter> = None;

		// Act
		let domain = proto_filter_to_domain(&proto_filter).unwrap();

		// Assert
		assert!(domain.source.is_none());
		assert!(domain.min_level.is_none());
		assert!(domain.since.is_none());
		assert!(domain.until.is_none());
		assert!(domain.search.is_none());
		assert!(domain.deployment_id.is_none());
	}

	// --- Filter with Some(empty LogFilter) -> all fields None ---

	#[rstest]
	fn test_filter_some_empty_all_none() {
		// Arrange
		let proto_filter = Some(pb::LogFilter {
			source: None,
			min_level: None,
			since: None,
			until: None,
			search: None,
			deployment_id: None,
		});

		// Act
		let domain = proto_filter_to_domain(&proto_filter).unwrap();

		// Assert
		assert!(domain.source.is_none());
		assert!(domain.min_level.is_none());
		assert!(domain.since.is_none());
		assert!(domain.until.is_none());
		assert!(domain.search.is_none());
		assert!(domain.deployment_id.is_none());
	}

	// --- Invalid metadata_json -> metadata=None ---

	#[rstest]
	fn test_proto_entry_invalid_metadata_json() {
		// Arrange
		let ts = fixed_timestamp();
		let proto = pb::LogEntry {
			timestamp: Some(to_proto_ts(ts)),
			level: pb::LogLevel::Info as i32,
			source: "app".to_string(),
			message: "test".to_string(),
			metadata_json: Some("not{json".to_string()),
		};

		// Act
		let domain = proto_entry_to_domain(&proto).unwrap();

		// Assert — invalid JSON is silently dropped (metadata=None)
		assert_eq!(domain.message, "test");
		assert!(domain.metadata.is_none());
	}

	#[rstest]
	fn test_proto_entry_missing_timestamp_returns_error() {
		// Arrange
		let proto = pb::LogEntry {
			timestamp: None,
			level: pb::LogLevel::Info as i32,
			source: "app".to_string(),
			message: "test".to_string(),
			metadata_json: None,
		};

		// Act
		let result = proto_entry_to_domain(&proto);

		// Assert
		assert!(result.is_err());
	}

	// --- Nested JSON metadata preserved ---

	#[rstest]
	fn test_nested_json_metadata_preserved() {
		// Arrange
		let ts = fixed_timestamp();
		let nested = serde_json::json!({
			"request": {
				"method": "POST",
				"headers": {"content-type": "application/json"},
				"body_size": 1024
			},
			"tags": ["slow", "auth"]
		});
		let entry = LogEntry {
			timestamp: ts,
			level: LogLevel::Debug,
			source: "middleware".to_string(),
			message: "request logged".to_string(),
			metadata: Some(nested.clone()),
		};

		// Act
		let proto = domain_entry_to_proto(&entry);
		let roundtripped = proto_entry_to_domain(&proto).unwrap();

		// Assert
		let meta = roundtripped.metadata.unwrap();
		assert_eq!(meta["request"]["method"], "POST");
		assert_eq!(
			meta["request"]["headers"]["content-type"],
			"application/json"
		);
		assert_eq!(meta["request"]["body_size"], 1024);
		assert_eq!(meta["tags"][0], "slow");
		assert_eq!(meta["tags"][1], "auth");
	}
}
