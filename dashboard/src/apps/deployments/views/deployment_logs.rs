//! Deployment logs view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{Filter, FilterOperator, FilterValue};
use reinhardt::di::Depends;
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Path, Response, StatusCode, get};
use reinhardt_cloud_proto::common::PaginationRequest;
use reinhardt_cloud_proto::log as log_pb;
use tracing::error;
use uuid::Uuid;

use crate::apps::deployments::models::Deployment;
use crate::apps::deployments::serializers::{DeploymentLogsResponse, LogEntry};
use crate::apps::organizations::permissions::{Action, require_permission};
use crate::config::GrpcChannelSingleton;

/// Maximum number of log entries returned per request.
///
/// The proto `PaginationRequest.page_size` field is declared as `uint64`,
/// which compiles to `u64` in the generated Rust types, so no cast is needed.
/// Capped at 100 to match the server-side `MAX_PAGE_SIZE` limit in
/// `reinhardt-cloud-core/src/pagination.rs`.
const LOGS_PAGE_SIZE: u64 = 100;

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

/// Retrieve logs for a specific deployment, scoped to the active organization.
///
/// Requires `Action::LogsRead` (Viewer or higher); returns 403 if the
/// caller's role does not permit the action.
///
/// Closes #419: the deployment lookup is now filtered by `organization_id`,
/// so a user can no longer fetch logs for a deployment belonging to a
/// different organization.
///
/// NOTE: The gRPC `LogFilter.source` field is still set to the deployment's
/// `app_name`. If two organizations create deployments with the same
/// `app_name`, the gRPC layer will return logs from both. Tightening that
/// filter to include `deployment_id` or `organization_id` is tracked
/// separately by the existing `// issue #390` follow-up.
#[get("/{id}/logs/", name = "logs")]
pub async fn deployment_logs(
	Path(id): Path<i64>,
	#[inject] AuthInfo(state): AuthInfo,
	#[inject] grpc_channel: Depends<GrpcChannelSingleton>,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;
	let organization_id = require_permission(user_id, Action::LogsRead).await?;

	// Verify the deployment exists and belongs to the active organization.
	// This is the fix for cross-tenant log leakage (#419): previously the
	// lookup filtered by `user_id`; now it filters by `organization_id`,
	// which correctly bounds visibility to the user's active org.
	let deployment = Deployment::objects()
		.filter(
			Deployment::field_id(),
			FilterOperator::Eq,
			FilterValue::Integer(id),
		)
		.filter(Filter::new(
			Deployment::field_organization_id(),
			FilterOperator::Eq,
			FilterValue::Integer(organization_id),
		))
		.first()
		.await
		.map_err(|e| {
			error!("Failed to retrieve deployment for logs: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::NotFound(format!("Deployment with id {id} not found")))?;

	// Fetch persisted logs via the gRPC LogService, filtering by app name.
	// The channel is resolved from DI as a shared, lazily-connected
	// singleton (see `crate::config::GrpcChannelSingleton`), so no TCP
	// connect happens on the request path until the first RPC.
	let mut client =
		log_pb::log_service_client::LogServiceClient::new(grpc_channel.channel.clone());

	// Filter by app_name (the log source field). Note: log isolation here
	// is by app_name, not by deployment id or organization. Two orgs that
	// happen to deploy apps with the same `app_name` would still see each
	// other's gRPC logs. Narrowing the LogFilter is tracked in issue #390.
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
		// Arrange -- a log entry with a valid Unix timestamp (2024-01-01 00:00:00 UTC)
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
		// Arrange -- nanos outside [0, 999_999_999] must be clamped to 0
		let entry = log_pb::LogEntry {
			timestamp: Some(prost_types::Timestamp {
				seconds: 1_704_067_200,
				nanos: 1_500_000_000,
			}),
			message: "clamped".to_string(),
			level: log_pb::LogLevel::Debug as i32,
			source: "svc".to_string(),
			metadata_json: None,
		};

		// Act -- must not panic
		let result = proto_entry_to_serializer(&entry);

		// Assert -- timestamp is still parseable (nanos clamped to 0)
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
