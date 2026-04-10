//! Deployment status update view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{Filter, FilterOperator, FilterValue};
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Json, Path, Response, StatusCode, post};
use tracing::error;
use uuid::Uuid;

use crate::apps::deployments::models::Deployment;
use crate::apps::deployments::serializers::{DeploymentResponse, DeploymentStatusRequest};

/// Update the status of a deployment (user-authenticated endpoint).
///
/// Accepts a status string. Returns the updated deployment.
/// Returns 404 if the deployment does not exist or is not owned
/// by the authenticated user.
#[post("/{id}/status/", name = "deployment_status")]
pub async fn deployment_status(
	Path(id): Path<i64>,
	Json(body): Json<DeploymentStatusRequest>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;

	let mut deployment = Deployment::objects()
		.filter("id", FilterOperator::Eq, FilterValue::Integer(id))
		.filter(Filter::new(
			Deployment::field_user_id(),
			FilterOperator::Eq,
			FilterValue::String(user_id.to_string()),
		))
		.first()
		.await
		.map_err(|e| {
			error!("Failed to retrieve deployment for status update: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::NotFound(format!("Deployment with id {id} not found")))?;

	deployment.status = body.status;

	let updated = Deployment::objects()
		.update(&deployment)
		.await
		.map_err(|e| {
			error!("Failed to update deployment status: {e}");
			AppError::Internal("Internal server error".to_string())
		})?;

	let resp = DeploymentResponse::from(updated);
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
