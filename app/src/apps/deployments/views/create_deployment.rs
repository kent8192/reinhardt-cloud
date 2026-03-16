//! Create deployment view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::http::{AuthState, ViewResult};
use reinhardt::{Request, Response, StatusCode, post};

use crate::apps::deployments::models::Deployment;
use crate::apps::deployments::serializers::{CreateDeploymentRequest, DeploymentResponse};

/// Create a new deployment (authentication required).
///
/// Workaround: Uses `AuthState::from_extensions` instead of `CurrentUser<User>`
/// DI injection because `CurrentUser` DB lookup requires complex DI configuration
/// that is not yet fully supported in the reinhardt-web test environment.
/// See: <https://github.com/kent8192/reinhardt-web/issues/2419>
#[post("/deployments/", name = "deployment_create")]
pub async fn create_deployment(request: Request) -> ViewResult<Response> {
	let auth_state = AuthState::from_extensions(&request.extensions);
	if !auth_state.is_some_and(|s| s.is_authenticated()) {
		return Err(AppError::Authentication(
			"Authentication required".to_string(),
		));
	}

	let body: CreateDeploymentRequest = request.json()?;
	let new_deployment = Deployment::new(
		body.app_name.clone(),
		body.cluster_id,
		"pending".to_string(),
		body.image.clone(),
	);
	let manager = Deployment::objects();
	let created = manager
		.create(&new_deployment)
		.await
		.map_err(|e| format!("{e}"))?;
	let resp = DeploymentResponse::from(created);
	Ok(Response::new(StatusCode::CREATED)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
