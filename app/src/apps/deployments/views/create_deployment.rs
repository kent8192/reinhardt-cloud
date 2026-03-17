//! Create deployment view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{FilterOperator, FilterValue};
use reinhardt::http::{AuthState, ViewResult};
use reinhardt::{Request, Response, StatusCode, post};
use tracing::error;
use uuid::Uuid;

use crate::apps::clusters::models::Cluster;
use crate::apps::deployments::models::Deployment;
use crate::apps::deployments::serializers::{CreateDeploymentRequest, DeploymentResponse};

/// Create a new deployment (authentication required).
///
/// Sets the deployment owner to the authenticated user.
///
/// Workaround: Uses `AuthState::from_extensions` instead of `CurrentUser<User>`
/// DI injection because `CurrentUser` DB lookup requires complex DI configuration
/// that is not yet fully supported in the reinhardt-web test environment.
/// See: <https://github.com/kent8192/reinhardt-web/issues/2419>
#[post("/deployments/", name = "deployment_create")]
pub async fn create_deployment(request: Request) -> ViewResult<Response> {
	let auth_state = AuthState::from_extensions(&request.extensions)
		.filter(|s| s.is_authenticated())
		.ok_or_else(|| AppError::Authentication("Authentication required".to_string()))?;
	let user_id = Uuid::parse_str(auth_state.user_id()).map_err(|e| {
		AppError::Internal(format!("Invalid user ID in token: {e}"))
	})?;

	let body: CreateDeploymentRequest = request.json()?;

	// Validate cluster exists before creating deployment
	let cluster_exists = Cluster::objects()
		.filter(
			Cluster::field_id(),
			FilterOperator::Eq,
			FilterValue::Integer(body.cluster_id),
		)
		.exists()
		.await
		.map_err(|e| {
			error!("Failed to check cluster existence: {e}");
			AppError::Internal("Internal server error".to_string())
		})?;

	if !cluster_exists {
		return Err(AppError::NotFound(format!(
			"Cluster with id {} not found",
			body.cluster_id
		)));
	}

	let new_deployment = Deployment::new(
		user_id,
		body.app_name.clone(),
		body.cluster_id,
		"pending".to_string(),
		body.image.clone(),
	);
	let manager = Deployment::objects();
	let created = manager.create(&new_deployment).await.map_err(|e| {
		error!("Failed to create deployment: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;
	let resp = DeploymentResponse::from(created);
	Ok(Response::new(StatusCode::CREATED)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
