//! Deployment logs view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{Filter, FilterOperator, FilterValue};
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Path, Response, StatusCode, get};
use tracing::error;
use uuid::Uuid;

use crate::apps::deployments::models::Deployment;
use crate::apps::deployments::serializers::DeploymentLogsResponse;

/// Retrieve logs for a specific deployment (authentication required).
///
/// Returns an empty log list as a placeholder until log persistence is
/// implemented. Returns 404 if the deployment does not exist or is not owned
/// by the authenticated user.
#[get("/deployments/{id}/logs/", name = "deployment_logs")]
pub async fn deployment_logs(
	Path(id): Path<i64>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;

	// Verify the deployment exists and belongs to the authenticated user
	let _deployment = Deployment::objects()
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

	// Placeholder: log persistence is not yet implemented
	let resp = DeploymentLogsResponse { logs: vec![] };
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
