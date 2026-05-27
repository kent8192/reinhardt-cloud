//! Create deployment view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Json, Path, Response, StatusCode, post};
use tracing::error;
use uuid::Uuid;

use crate::apps::clusters::models::Cluster;
use crate::apps::deployments::models::Deployment;
use crate::apps::deployments::serializers::{CreateDeploymentRequest, DeploymentResponse};
use crate::apps::organizations::permissions::{Action, require_permission_for_org};

/// Create a new deployment (authentication required).
///
/// Requires `Action::DeploymentCreate` (Developer or higher); Viewers
/// receive 403. Sets the deployment owner to the specified organization.
/// Validates that the target cluster belongs to the same organization.
#[post("/orgs/{org}/deployments/", name = "create")]
pub async fn create_deployment(
	Path(org_slug): Path<String>,
	Json(body): Json<CreateDeploymentRequest>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;
	let organization_id =
		require_permission_for_org(user_id, &org_slug, Action::DeploymentCreate).await?;

	// Validate cluster exists and belongs to the specified organization.
	let cluster_exists = Cluster::objects()
		.filter(Cluster::field_id().eq(body.cluster_id))
		.filter(Cluster::field_organization_id().eq(organization_id))
		.exists()
		.await
		.map_err(|e| {
			error!("Failed to check cluster existence: {e}");
			AppError::Internal("Internal server error".to_string())
		})?;

	if !cluster_exists {
		return Err(AppError::NotFound(format!(
			"Cluster with id {} not found",
			body.cluster_id
		)));
	}

	let new_deployment = Deployment::new(
		organization_id,
		body.app_name.clone(),
		body.cluster_id,
		"pending".to_string(),
		body.image.clone(),
	);
	let manager = Deployment::objects();
	let created = manager.create(&new_deployment).await.map_err(|e| {
		error!("Failed to create deployment: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;
	let resp = DeploymentResponse::from(created);
	Ok(Response::new(StatusCode::CREATED)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
