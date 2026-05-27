//! List deployments view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Path, Query, Response, StatusCode, get};
use reinhardt_cloud_core::pagination::{PaginatedResponse, PaginationParams};
use tracing::error;
use uuid::Uuid;

use crate::apps::deployments::models::Deployment;
use crate::apps::deployments::serializers::DeploymentResponse;
use crate::apps::organizations::permissions::{Action, require_permission_for_org};

/// List deployments for an organization with pagination.
///
/// Requires `Action::DeploymentRead` (Viewer or higher).
#[get("/orgs/{org}/deployments/", name = "list")]
pub async fn list_deployments(
	Path(org_slug): Path<String>,
	Query(params): Query<PaginationParams>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;
	let organization_id =
		require_permission_for_org(user_id, &org_slug, Action::DeploymentRead).await?;

	let total = Deployment::objects()
		.filter(Deployment::field_organization_id().eq(organization_id))
		.count()
		.await
		.map_err(|e| {
			error!("Failed to count deployments: {e}");
			AppError::Internal("Internal server error".to_string())
		})? as u64;
	let offset: usize = params.offset().try_into().unwrap_or(0);
	let limit: usize = params.page_size().try_into().unwrap_or(20).min(100);
	let deployments = Deployment::objects()
		.filter(Deployment::field_organization_id().eq(organization_id))
		.order_by(&["id"])
		.offset(offset)
		.limit(limit)
		.all()
		.await
		.map_err(|e| {
			error!("Failed to list deployments: {e}");
			AppError::Internal("Internal server error".to_string())
		})?;
	let items: Vec<DeploymentResponse> = deployments
		.into_iter()
		.map(DeploymentResponse::from)
		.collect();
	let paginated = PaginatedResponse::new(items, total, &params);

	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&paginated)?))
}
