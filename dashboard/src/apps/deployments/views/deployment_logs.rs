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
const LOGS_PAGE_SIZE: u64 = 200;

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
			chrono::DateTime::from_timestamp(t.seconds, nanos)
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
	let mut client = log_pb::log_service_client::LogServiceClient::connect(grpc_endpoint())
		.await
		.map_err(|e| {
			error!("Failed to connect to gRPC LogService: {e}");
			AppError::Internal("Log service unavailable".to_string())
		})?;

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
