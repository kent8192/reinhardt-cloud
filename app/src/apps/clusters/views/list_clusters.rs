//! List clusters view.

use nuages_core::pagination::{PaginatedResponse, PaginationParams};
use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{FilterOperator, FilterValue};
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Query, Response, StatusCode, get};
use tracing::error;
use uuid::Uuid;

use crate::apps::clusters::models::Cluster;
use crate::apps::clusters::serializers::ClusterResponse;

/// List clusters owned by the authenticated user with pagination.
///
/// Accepts optional query parameters `page` and `page_size` for pagination.
/// Returns a paginated response with items, total count, and page metadata.
#[get("/clusters/", name = "cluster_list")]
pub async fn list_clusters(
	Query(params): Query<PaginationParams>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;

	let total = Cluster::objects()
		.filter(
			Cluster::field_user_id(),
			FilterOperator::Eq,
			FilterValue::String(user_id.to_string()),
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
			Cluster::field_user_id(),
			FilterOperator::Eq,
			FilterValue::String(user_id.to_string()),
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
