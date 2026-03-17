//! List deployments view.

use nuages_core::pagination::{PaginatedResponse, PaginationParams};
use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{FilterOperator, FilterValue};
use reinhardt::http::{AuthState, ViewResult};
use reinhardt::{Request, Response, StatusCode, get};
use uuid::Uuid;

use crate::apps::deployments::models::Deployment;
use crate::apps::deployments::serializers::DeploymentResponse;

/// List deployments owned by the authenticated user with pagination.
///
/// Accepts optional query parameters `page` and `page_size` for pagination.
/// Returns a paginated response with items, total count, and page metadata.
///
/// Workaround: Uses `AuthState::from_extensions` instead of `CurrentUser<User>`
/// DI injection because `CurrentUser` DB lookup requires complex DI configuration
/// that is not yet fully supported in the reinhardt-web test environment.
/// See: <https://github.com/kent8192/reinhardt-web/issues/2419>
#[get("/deployments/", name = "deployment_list")]
pub async fn list_deployments(request: Request) -> ViewResult<Response> {
	let auth_state = AuthState::from_extensions(&request.extensions)
		.filter(|s| s.is_authenticated())
		.ok_or_else(|| AppError::Authentication("Authentication required".to_string()))?;
	let user_id = Uuid::parse_str(auth_state.user_id())
		.map_err(|e| AppError::Internal(format!("Invalid user ID in token: {e}")))?;

	let params: PaginationParams = request
		.query_as()
		.map_err(|e| AppError::Validation(format!("Invalid pagination parameters: {e}")))?;

	let total = Deployment::objects()
		.filter(
			Deployment::field_user_id(),
			FilterOperator::Eq,
			FilterValue::String(user_id.to_string()),
		)
		.count()
		.await
		.map_err(|e| format!("{e}"))? as u64;
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
		.map_err(|e| format!("{e}"))?;
	let items: Vec<DeploymentResponse> = deployments
		.into_iter()
		.map(DeploymentResponse::from)
		.collect();
	let paginated = PaginatedResponse::new(items, total, &params);

	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&paginated)?))
}
