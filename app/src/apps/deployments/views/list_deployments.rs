//! List deployments view.

use reinhardt::core::serde::json;
use reinhardt::http::ViewResult;
use reinhardt::Model;
use reinhardt::{Response, StatusCode, get};

use crate::apps::deployments::models::Deployment;
use crate::apps::deployments::serializers::DeploymentResponse;

/// List all deployments.
#[get("/deployments/", name = "deployment_list")]
pub async fn list_deployments() -> ViewResult<Response> {
	let manager = Deployment::objects();
	let deployments = manager.all().all().await.map_err(|e| format!("{e}"))?;
	let responses: Vec<DeploymentResponse> =
		deployments.into_iter().map(DeploymentResponse::from).collect();
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&responses)?))
}
