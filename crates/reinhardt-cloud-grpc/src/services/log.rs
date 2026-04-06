//! gRPC Log Service server and client implementations.

use std::pin::Pin;
use std::sync::Arc;

use prost_types::Timestamp;
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status, Streaming};

use reinhardt_cloud_core::pagination::PaginationParams;
use reinhardt_cloud_core::traits::LogService;
use reinhardt_cloud_proto::common::{PaginationResponse, StatusResponse};
use reinhardt_cloud_proto::log as pb;
use reinhardt_cloud_types::log::{LogEntry, LogFilter, LogLevel};

// --- Conversions ---

fn timestamp_from_chrono(dt: chrono::DateTime<chrono::Utc>) -> Option<Timestamp> {
	Some(Timestamp {
		seconds: dt.timestamp(),
		nanos: dt.timestamp_subsec_nanos() as i32,
	})
}

fn proto_timestamp_to_chrono(ts: Option<Timestamp>) -> chrono::DateTime<chrono::Utc> {
	ts.and_then(|t| {
		chrono::DateTime::from_timestamp(t.seconds, t.nanos.try_into().unwrap_or(0))
	})
	.unwrap_or_else(chrono::Utc::now)
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

fn proto_entry_to_domain(entry: &pb::LogEntry) -> LogEntry {
	LogEntry {
		timestamp: proto_timestamp_to_chrono(entry.timestamp.clone()),
		level: proto_level_to_domain(entry.level),
		source: entry.source.clone(),
		message: entry.message.clone(),
		metadata: entry
			.metadata_json
			.as_ref()
			.and_then(|s| serde_json::from_str(s).ok()),
	}
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

fn proto_filter_to_domain(filter: &Option<pb::LogFilter>) -> LogFilter {
	match filter {
		Some(f) => LogFilter {
			source: f.source.clone(),
			min_level: f.min_level.map(proto_level_to_domain),
			since: f.since.clone().map(|t| proto_timestamp_to_chrono(Some(t))),
			until: f.until.clone().map(|t| proto_timestamp_to_chrono(Some(t))),
			search: f.search.clone(),
		},
		None => LogFilter::default(),
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
			let entries: Vec<LogEntry> = push_req.entries.iter().map(proto_entry_to_domain).collect();
			total += entries.len() as u64;
			self.service
				.push_logs(entries)
				.await
				.map_err(|e| Status::internal(e.to_string()))?;
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
		let filter = proto_filter_to_domain(&request.into_inner().filter);

		let stream = self
			.service
			.tail_logs(filter)
			.await
			.map_err(|e| Status::internal(e.to_string()))?;

		let mapped = stream.map(|result| match result {
			Ok(entry) => Ok(domain_entry_to_proto(&entry)),
			Err(e) => Err(Status::internal(e.to_string())),
		});

		Ok(Response::new(Box::pin(mapped)))
	}

	async fn list_logs(
		&self,
		request: Request<pb::ListLogsRequest>,
	) -> Result<Response<pb::ListLogsResponse>, Status> {
		let req = request.into_inner();
		let filter = proto_filter_to_domain(&req.filter);
		let pagination = req
			.pagination
			.map(|p| PaginationParams::new(Some(p.page), Some(p.page_size)))
			.unwrap_or_default();

		let result = self
			.service
			.list_logs(filter, pagination)
			.await
			.map_err(|e| Status::internal(e.to_string()))?;

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
