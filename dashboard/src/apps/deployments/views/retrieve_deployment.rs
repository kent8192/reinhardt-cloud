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
/// /// Retrieve a single deployment by ID, scoped to the specified organization.
///
/// Requires `Action::DeploymentRead` (Viewer or higher); returns 403 if
/// the caller's role does not permit the action. Returns 404 if the
/// deployment does not exist or does not belong to the specified organization.
#[get("/orgs/{org}/deployments/{deployment_id}/", name = "retrieve")]
pub async fn retrieve_deployment(
	Path((deployment_id, org)): Path<(i64, String)>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;
	let organization_id =
		require_permission_for_org(user_id, &org, Action::DeploymentRead).await?;

	let deployment = Deployment::objects()
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
			error!("Failed to retrieve deployment: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::NotFound(format!("Deployment with id {deployment_id} not found")))?;

	let resp = DeploymentResponse::from(deployment);
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
