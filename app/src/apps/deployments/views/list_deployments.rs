//! List deployments view.

use nuages_core::pagination::{PaginatedResponse, PaginationParams};
use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::http::{AuthState, ViewResult};
use reinhardt::{Request, Response, StatusCode, get};

use crate::apps::deployments::models::Deployment;
use crate::apps::deployments::serializers::DeploymentResponse;

/// List deployments with pagination (authentication required).
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
	let auth_state = AuthState::from_extensions(&request.extensions);
	if !auth_state.is_some_and(|s| s.is_authenticated()) {
		return Err(AppError::Authentication(
			"Authentication required".to_string(),
		));
	}

	let params: PaginationParams = request.query_as().unwrap_or_default();

	let total = Deployment::objects()
		.all()
		.count()
		.await
		.map_err(|e| format!("{e}"))? as u64;
	let deployments = Deployment::objects()
		.all()
		.offset(params.offset() as usize)
		.limit(params.page_size() as usize)
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
