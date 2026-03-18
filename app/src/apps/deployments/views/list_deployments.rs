//! List deployments view.

use nuages_core::pagination::{PaginatedResponse, PaginationParams};
use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{FilterOperator, FilterValue};
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Query, Response, StatusCode, get};
use tracing::error;
use uuid::Uuid;

use crate::apps::deployments::models::Deployment;
use crate::apps::deployments::serializers::DeploymentResponse;

/// List deployments owned by the authenticated user with pagination.
///
/// Accepts optional query parameters `page` and `page_size` for pagination.
/// Returns a paginated response with items, total count, and page metadata.
#[get("/deployments/", name = "deployment_list", use_inject = true)]
pub async fn list_deployments(
	Query(params): Query<PaginationParams>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;

	let total = Deployment::objects()
		.filter(
			Deployment::field_user_id(),
			FilterOperator::Eq,
			FilterValue::String(user_id.to_string()),
		)
		.count()
		.await
		.map_err(|e| {
			error!("Failed to count deployments: {e}");
			AppError::Internal("Internal server error".to_string())
		})? as u64;
	let offset: usize = params.offset().try_into().unwrap_or(usize::MAX);
	let limit: usize = params.page_size().try_into().unwrap_or(usize::MAX);
	let deployments = Deployment::objects()
		.filter(
			Deployment::field_user_id(),
			FilterOperator::Eq,
			FilterValue::String(user_id.to_string()),
		)
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
