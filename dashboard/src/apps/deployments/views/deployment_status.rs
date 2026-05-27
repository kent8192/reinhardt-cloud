//! Deployment status update view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Json, Path, Response, StatusCode, post};
use tracing::error;
use uuid::Uuid;

use crate::apps::deployments::models::Deployment;
use crate::apps::deployments::serializers::{DeploymentResponse, DeploymentStatusRequest};
use crate::apps::organizations::permissions::{Action, require_permission_for_org};

/// Update the status of a deployment (user-authenticated endpoint).
///
/// Requires `Action::DeploymentUpdate` (Developer or higher); Viewers
/// receive 403. Accepts a status string. Returns the updated deployment.
/// Returns 404 if the deployment does not exist or does not belong to the
/// specified organization.
#[post("/orgs/{org}/deployments/{deployment_id}/status/", name = "status")]
pub async fn deployment_status(
	Path((org, deployment_id)): Path<(String, i64)>,
	Json(body): Json<DeploymentStatusRequest>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;
	let organization_id =
		require_permission_for_org(user_id, &org, Action::DeploymentUpdate).await?;

	let mut deployment = Deployment::objects()
		.filter(Deployment::field_id().eq(deployment_id))
		.filter(Deployment::field_organization_id().eq(organization_id))
		.first()
		.await
		.map_err(|e| {
			error!("Failed to retrieve deployment for status update: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::NotFound(format!("Deployment with id {deployment_id} not found")))?;

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
