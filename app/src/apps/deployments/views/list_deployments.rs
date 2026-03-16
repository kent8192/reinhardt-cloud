//! List deployments view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::http::{AuthState, ViewResult};
use reinhardt::{Request, Response, StatusCode, get};

use crate::apps::deployments::models::Deployment;
use crate::apps::deployments::serializers::DeploymentResponse;

/// List all deployments (authentication required).
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

	let manager = Deployment::objects();
	let deployments = manager.all().all().await.map_err(|e| format!("{e}"))?;
	let responses: Vec<DeploymentResponse> = deployments
		.into_iter()
		.map(DeploymentResponse::from)
		.collect();
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&responses)?))
}
