//! List clusters view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{FilterOperator, FilterValue};
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Path, Query, Response, StatusCode, get};
use reinhardt_cloud_core::pagination::{PaginatedResponse, PaginationParams};
use tracing::error;
use uuid::Uuid;

use crate::apps::clusters::models::Cluster;
use crate::apps::clusters::serializers::ClusterResponse;
use crate::apps::organizations::permissions::{Action, require_permission_for_org};

/// List clusters for an organization with pagination.
///
/// Accepts optional query parameters `page` and `page_size` for pagination.
/// Returns a paginated response with items, total count, and page metadata.
///
/// Requires `Action::ClusterRead` (Viewer or higher).
#[get("/orgs/{org}/clusters/", name = "list")]
pub async fn list_clusters(
	Path(org_slug): Path<String>,
	Query(params): Query<PaginationParams>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;
	let organization_id =
		require_permission_for_org(user_id, &org_slug, Action::ClusterRead).await?;

	let total = Cluster::objects()
		.filter(
			Cluster::field_organization_id(),
			FilterOperator::Eq,
			FilterValue::Integer(organization_id),
		)
		.count()
		.await
		.map_err(|e| {
			error!("Failed to count clusters: {e}");
			AppError::Internal("Internal server error".to_string())
		})? as u64;
	let offset: usize = params.offset().try_into().unwrap_or(0);
	let limit: usize = params.page_size().try_into().unwrap_or(20).min(100);
	let clusters = Cluster::objects()
		.filter(
			Cluster::field_organization_id(),
			FilterOperator::Eq,
			FilterValue::Integer(organization_id),
		)
		.order_by(&["id"])
		.offset(offset)
		.limit(limit)
		.all()
		.await
		.map_err(|e| {
			error!("Failed to list clusters: {e}");
			AppError::Internal("Internal server error".to_string())
		})?;
	let items: Vec<ClusterResponse> = clusters.into_iter().map(ClusterResponse::from).collect();
	let paginated = PaginatedResponse::new(items, total, &params);

	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&paginated)?))
}
