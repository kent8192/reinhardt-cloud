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
use crate::apps::organizations::helpers::current_organization_id_for_user;

/// Update an existing deployment (authentication required).
///
/// Accepts optional fields; only provided fields are applied.
/// Returns 400 if the request body is empty (no fields provided).
/// Returns 404 if the deployment does not exist or does not belong to the
/// authenticated user's active organization.
#[put("/{id}/", name = "update")]
pub async fn update_deployment(
	Path(id): Path<i64>,
	Json(body): Json<UpdateDeploymentRequest>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;
	let organization_id = current_organization_id_for_user(user_id).await?;

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
			error!("Failed to retrieve deployment for update: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::NotFound(format!("Deployment with id {id} not found")))?;

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
