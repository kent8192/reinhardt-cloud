//! Update deployment view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{Filter, FilterOperator, FilterValue};
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Json, Path, Response, StatusCode, Validate, put};
use tracing::error;
use uuid::Uuid;

use crate::apps::deployments::models::Deployment;
use crate::apps::deployments::serializers::{DeploymentResponse, UpdateDeploymentRequest};
use crate::apps::organizations::permissions::{Action, require_permission_for_org};

/// Workaround for kent8192/reinhardt-web#4013 (tracked in reinhardt-cloud#466)
/// Remove this comment when the upstream issue is resolved.
///
/// Ideal implementation (without workaround):
///   `Path((org, deployment_id)): Path<(String, i64)>` — URL pattern order
///
/// `Path<(T1, T2)>` sorts parameters alphabetically by name, not URL order.
/// `deployment_id` < `org` alphabetically → tuple[0] is the id, tuple[1] is the org.
///
/// /// Update an existing deployment (authentication required).
///
/// Requires `Action::DeploymentUpdate` (Developer or higher); Viewers
/// receive 403. Accepts optional fields; only provided fields are
/// applied. Returns 400 if the request body is empty (no fields
/// provided). Returns 404 if the deployment does not exist or does not
/// belong to the specified organization.
#[put("/orgs/{org}/deployments/{deployment_id}/", name = "update")]
pub async fn update_deployment(
	Path((deployment_id, org)): Path<(i64, String)>,
	Json(body): Json<UpdateDeploymentRequest>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;
	let organization_id =
		require_permission_for_org(user_id, &org, Action::DeploymentUpdate).await?;

	// Validate the request body
	body.validate()?;

	// Reject empty updates -- at least one field must be provided
	if body.app_name.is_none() && body.image.is_none() && body.status.is_none() {
		return Err(AppError::Validation(
			"At least one field must be provided for update".to_string(),
		));
	}

	let mut deployment = Deployment::objects()
		.filter(
			Deployment::field_id(),
			FilterOperator::Eq,
			FilterValue::Integer(deployment_id),
		)
		.filter(Filter::new(
			Deployment::field_organization_id(),
			FilterOperator::Eq,
			FilterValue::Integer(organization_id),
		))
		.first()
		.await
		.map_err(|e| {
			error!("Failed to retrieve deployment for update: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::NotFound(format!("Deployment with id {deployment_id} not found")))?;

	if let Some(app_name) = body.app_name {
		deployment.app_name = app_name;
	}
	if let Some(image) = body.image {
		deployment.image = image;
	}
	if let Some(status) = body.status {
		deployment.status = status;
	}

	let updated = Deployment::objects()
		.update(&deployment)
		.await
		.map_err(|e| {
			error!("Failed to update deployment: {e}");
			AppError::Internal("Internal server error".to_string())
		})?;

	let resp = DeploymentResponse::from(updated);
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
