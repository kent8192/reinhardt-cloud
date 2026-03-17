//! List clusters view.

use nuages_core::pagination::{PaginatedResponse, PaginationParams};
use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::http::{AuthState, ViewResult};
use reinhardt::{Request, Response, StatusCode, get};

use crate::apps::clusters::models::Cluster;
use crate::apps::clusters::serializers::ClusterResponse;

/// List clusters with pagination (authentication required).
///
/// Accepts optional query parameters `page` and `page_size` for pagination.
/// Returns a paginated response with items, total count, and page metadata.
///
/// Workaround: Uses `AuthState::from_extensions` instead of `CurrentUser<User>`
/// DI injection because `CurrentUser` DB lookup requires complex DI configuration
/// that is not yet fully supported in the reinhardt-web test environment.
/// See: <https://github.com/kent8192/reinhardt-web/issues/2419>
#[get("/clusters/", name = "cluster_list")]
pub async fn list_clusters(request: Request) -> ViewResult<Response> {
	let auth_state = AuthState::from_extensions(&request.extensions);
	if !auth_state.is_some_and(|s| s.is_authenticated()) {
		return Err(AppError::Authentication(
			"Authentication required".to_string(),
		));
	}

	let params: PaginationParams = request
		.query_as()
		.map_err(|e| AppError::Validation(format!("Invalid pagination parameters: {e}")))?;

	let total = Cluster::objects()
		.all()
		.count()
		.await
		.map_err(|e| format!("{e}"))? as u64;
	let offset: usize = params.offset().try_into().unwrap_or(usize::MAX);
	let limit: usize = params.page_size().try_into().unwrap_or(usize::MAX);
	let clusters = Cluster::objects()
		.all()
		.order_by(&["id"])
		.offset(offset)
		.limit(limit)
		.all()
		.await
		.map_err(|e| format!("{e}"))?;
	let items: Vec<ClusterResponse> = clusters.into_iter().map(ClusterResponse::from).collect();
	let paginated = PaginatedResponse::new(items, total, &params);

	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&paginated)?))
}
