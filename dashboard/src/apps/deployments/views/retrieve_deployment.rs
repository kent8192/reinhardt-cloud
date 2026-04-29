//! Retrieve deployment view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{Filter, FilterOperator, FilterValue};
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Path, Response, StatusCode, get};
use tracing::error;
use uuid::Uuid;

use crate::apps::deployments::models::Deployment;
use crate::apps::deployments::serializers::DeploymentResponse;
use crate::apps::organizations::permissions::{Action, require_permission};

/// Retrieve a single deployment by ID, scoped to the active organization.
///
/// Requires `Action::DeploymentRead` (Viewer or higher); returns 403 if
/// the caller's role does not permit the action. Returns 404 if the
/// deployment does not exist or does not belong to the authenticated
/// user's active organization.
#[get("/{id}/", name = "retrieve")]
pub async fn retrieve_deployment(
	Path(id): Path<i64>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;
	let organization_id = require_permission(user_id, Action::DeploymentRead).await?;

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
			error!("Failed to retrieve deployment: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::NotFound(format!("Deployment with id {id} not found")))?;

	let resp = DeploymentResponse::from(deployment);
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
