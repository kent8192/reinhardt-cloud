//! Deployment logs view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{Filter, FilterOperator, FilterValue};
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Path, Response, StatusCode, get};
use reinhardt_cloud_proto::common::PaginationRequest;
use reinhardt_cloud_proto::log as log_pb;
use tracing::error;
use uuid::Uuid;

use crate::apps::deployments::models::Deployment;
use crate::apps::deployments::serializers::{DeploymentLogsResponse, LogEntry};

/// Default gRPC endpoint used when `GRPC_ENDPOINT` is not set.
const DEFAULT_GRPC_ENDPOINT: &str = "http://127.0.0.1:50051";

/// Maximum number of log entries returned per request.
///
/// The proto `PaginationRequest.page_size` field is declared as `uint64`,
/// which compiles to `u64` in the generated Rust types, so no cast is needed.
/// Capped at 100 to match the server-side `MAX_PAGE_SIZE` limit in
/// `reinhardt-cloud-core/src/pagination.rs`.
const LOGS_PAGE_SIZE: u64 = 100;

/// Resolve the gRPC endpoint from the environment or fall back to the default.
fn grpc_endpoint() -> String {
	std::env::var("GRPC_ENDPOINT").unwrap_or_else(|_| DEFAULT_GRPC_ENDPOINT.to_string())
}

/// Map a proto log level to its lowercase string form.
fn proto_level_to_str(level: i32) -> &'static str {
	match log_pb::LogLevel::try_from(level) {
		Ok(log_pb::LogLevel::Debug) => "debug",
		Ok(log_pb::LogLevel::Warn) => "warn",
		Ok(log_pb::LogLevel::Error) => "error",
		_ => "info",
	}
}

/// Convert a proto `LogEntry` into the dashboard's serializer shape.
fn proto_entry_to_serializer(entry: &log_pb::LogEntry) -> LogEntry {
	let timestamp = entry
		.timestamp
		.map(|t| {
			let nanos = if (0..=999_999_999).contains(&t.nanos) {
				t.nanos as u32
			} else {
				0
			};
			chrono::DateTime::<chrono::Utc>::from_timestamp(t.seconds, nanos)
				.map(|dt| dt.to_rfc3339())
				.unwrap_or_default()
		})
		.unwrap_or_default();

	LogEntry {
		timestamp,
		message: entry.message.clone(),
		level: proto_level_to_str(entry.level).to_string(),
	}
}

/// Retrieve logs for a specific deployment (authentication required).
///
/// Logs are fetched from the `LogService.ListLogs` gRPC RPC using the
/// deployment's `app_name` as the log `source` filter. Returns 404 if the
/// deployment does not exist or is not owned by the authenticated user.
#[get("/{id}/logs/", name = "logs")]
pub async fn deployment_logs(
	Path(id): Path<i64>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;

	// Verify the deployment exists and belongs to the authenticated user.
	let deployment = Deployment::objects()
		.filter(
			Deployment::field_id(),
			FilterOperator::Eq,
			FilterValue::Integer(id),
		)
		.filter(Filter::new(
			Deployment::field_user_id(),
			FilterOperator::Eq,
			FilterValue::String(user_id.to_string()),
		))
		.first()
		.await
		.map_err(|e| {
			error!("Failed to retrieve deployment for logs: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::NotFound(format!("Deployment with id {id} not found")))?;

	// Fetch persisted logs via the gRPC LogService, filtering by app name.
	// NOTE: This creates a new gRPC connection on every HTTP request. A
	// future refactor should inject a shared `tonic::transport::Channel`
	// via DI (see reinhardt-cloud#382) to amortise connection overhead.
	let mut client = log_pb::log_service_client::LogServiceClient::connect(grpc_endpoint())
		.await
		.map_err(|e| {
			error!("Failed to connect to gRPC LogService: {e}");
			AppError::Internal("Log service unavailable".to_string())
		})?;

	// Filter by app_name (the log source field). Note: log isolation is by
	// app_name, not by deployment id or owner; cross-tenant leakage is possible
	// if two users share the same app_name. A stricter filter (e.g. by a
	// globally unique deployment UUID) is tracked in issue #390.
	let request = log_pb::ListLogsRequest {
		filter: Some(log_pb::LogFilter {
			source: Some(deployment.app_name.clone()),
			..Default::default()
		}),
		pagination: Some(PaginationRequest {
			page: 1,
			page_size: LOGS_PAGE_SIZE,
		}),
	};

	let response = client.list_logs(request).await.map_err(|e| {
		error!("gRPC ListLogs call failed: {e}");
		AppError::Internal("Failed to retrieve deployment logs".to_string())
	})?;

	let logs = response
		.into_inner()
		.entries
		.iter()
		.map(proto_entry_to_serializer)
		.collect();

	let resp = DeploymentLogsResponse { logs };
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}

#[cfg(test)]
mod tests {
	use super::*;
	use reinhardt_cloud_proto::log as log_pb;
	use rstest::rstest;

	#[rstest]
	#[case(log_pb::LogLevel::Debug as i32, "debug")]
	#[case(log_pb::LogLevel::Warn as i32, "warn")]
	#[case(log_pb::LogLevel::Error as i32, "error")]
	#[case(log_pb::LogLevel::Info as i32, "info")]
	#[case(-1, "info")]
	fn test_proto_level_to_str(#[case] level: i32, #[case] expected: &str) {
		assert_eq!(proto_level_to_str(level), expected);
	}

	#[rstest]
	fn test_proto_entry_to_serializer_valid_timestamp() {
		// Arrange — a log entry with a valid Unix timestamp (2024-01-01 00:00:00 UTC)
		let entry = log_pb::LogEntry {
			timestamp: Some(prost_types::Timestamp {
				seconds: 1_704_067_200,
				nanos: 0,
			}),
			message: "hello world".to_string(),
			level: log_pb::LogLevel::Info as i32,
			source: "svc".to_string(),
			metadata_json: None,
		};

		// Act
		let result = proto_entry_to_serializer(&entry);

		// Assert
		assert_eq!(result.message, "hello world");
		assert_eq!(result.level, "info");
		assert!(
			result.timestamp.starts_with("2024-01-01"),
			"expected RFC3339 date, got: {}",
			result.timestamp
		);
	}

	#[rstest]
	fn test_proto_entry_to_serializer_out_of_range_nanos_clamped() {
		// Arrange — nanos outside [0, 999_999_999] must be clamped to 0
		let entry = log_pb::LogEntry {
			timestamp: Some(prost_types::Timestamp {
				seconds: 1_704_067_200,
				nanos: 1_500_000_000, // out of range
			}),
			message: "clamped".to_string(),
			level: log_pb::LogLevel::Debug as i32,
			source: "svc".to_string(),
			metadata_json: None,
		};

		// Act — must not panic
		let result = proto_entry_to_serializer(&entry);

		// Assert — timestamp is still parseable (nanos clamped to 0)
		assert!(!result.timestamp.is_empty());
	}

	#[rstest]
	fn test_proto_entry_to_serializer_missing_timestamp_yields_empty() {
		// Arrange
		let entry = log_pb::LogEntry {
			timestamp: None,
			message: "no-ts".to_string(),
			level: log_pb::LogLevel::Info as i32,
			source: "svc".to_string(),
			metadata_json: None,
		};

		// Act
		let result = proto_entry_to_serializer(&entry);

		// Assert
		assert_eq!(result.timestamp, "");
	}
}
