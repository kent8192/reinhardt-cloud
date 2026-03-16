//! Create deployment view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::http::ViewResult;
use reinhardt::{CurrentUser, Json, Response, StatusCode, post};

use crate::apps::auth::models::User;
use crate::apps::deployments::models::Deployment;
use crate::apps::deployments::serializers::{CreateDeploymentRequest, DeploymentResponse};

/// Create a new deployment (authentication required).
#[post(
	"/deployments/",
	name = "deployment_create",
	pre_validate = true,
	use_inject = true
)]
pub async fn create_deployment(
	body: Json<CreateDeploymentRequest>,
	#[inject] user: CurrentUser<User>,
) -> ViewResult<Response> {
	if !user.is_authenticated() {
		return Err(AppError::Authentication(
			"Authentication required".to_string(),
		));
	}

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
