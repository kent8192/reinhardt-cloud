//! Delete deployment view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::db::orm::{Filter, FilterOperator, FilterValue};
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Path, Response, StatusCode, delete};
use tracing::error;
use uuid::Uuid;

use crate::apps::deployments::models::Deployment;
use crate::apps::organizations::permissions::{Action, require_permission};

/// Delete a deployment by ID (authentication required).
///
/// Requires `Action::DeploymentDelete` (Developer or higher); Viewers
/// receive 403. Returns 204 No Content on success, 404 if the deployment
/// does not exist or does not belong to the authenticated user's active
/// organization.
#[delete("/{id}/", name = "delete")]
pub async fn delete_deployment(
	Path(id): Path<i64>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;
	let organization_id = require_permission(user_id, Action::DeploymentDelete).await?;

	Deployment::objects()
		.filter("id", FilterOperator::Eq, FilterValue::Integer(id))
		.filter(Filter::new(
			Deployment::field_organization_id(),
			FilterOperator::Eq,
			FilterValue::Integer(organization_id),
		))
		.first()
		.await
		.map_err(|e| {
			error!("Failed to retrieve deployment for deletion: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::NotFound(format!("Deployment with id {id} not found")))?;

	// Use path id directly for deletion -- the ownership check above
	// already confirmed the record exists and belongs to this organization
	Deployment::objects().delete(id).await.map_err(|e| {
		error!("Failed to delete deployment: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	Ok(Response::new(StatusCode::NO_CONTENT))
}
