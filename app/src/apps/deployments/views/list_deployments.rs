//! List deployments view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::http::ViewResult;
use reinhardt::{CurrentUser, Response, StatusCode, get};

use crate::apps::auth::models::User;
use crate::apps::deployments::models::Deployment;
use crate::apps::deployments::serializers::DeploymentResponse;

/// List all deployments (authentication required).
#[get("/deployments/", name = "deployment_list", use_inject = true)]
pub async fn list_deployments(#[inject] user: CurrentUser<User>) -> ViewResult<Response> {
	if !user.is_authenticated() {
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
