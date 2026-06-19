//! Server routes for deployment endpoints.

use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::Model;
use reinhardt::di::Depends;
use reinhardt::http::ViewResult;
use reinhardt::reinhardt_params::Body;
use reinhardt::{CurrentUser, Response, StatusCode, Validate, post};
use serde::Serialize;
use tracing::error;

use crate::apps::auth::models::User;
use crate::apps::clusters::models::Cluster;
use crate::apps::deployments::serializers::{CliDeploymentRequest, CliDeploymentResponse};
use crate::apps::deployments::services::{
	SubmitProjectDeploymentError, SubmitProjectDeploymentInput, submit_project_deployment,
};
use crate::apps::organizations::helpers::current_organization_id_for_user;
use crate::config::{AgentRegistrySingleton, AgentRegistrySingletonKey};

#[derive(Debug, Serialize)]
struct ApiErrorResponse<'a> {
	error: &'a str,
}

/// Submit a CLI-generated Project manifest through the Dashboard control plane.
#[post("/deployments/cli/", name = "cli-deploy")]
pub async fn cli_deploy(
	Body(payload): Body,
	#[inject] CurrentUser(user): CurrentUser<User>,
	#[inject] agent_registry: Depends<AgentRegistrySingletonKey, AgentRegistrySingleton>,
) -> ViewResult<Response> {
	let mut request: CliDeploymentRequest = match serde_json::from_slice(&payload) {
		Ok(request) => request,
		Err(err) => {
			let message = format!("Invalid CLI deployment request JSON: {err}");
			return json_response(
				StatusCode::BAD_REQUEST,
				ApiErrorResponse { error: &message },
			);
		}
	};
	trim_request(&mut request);
	if let Err(errors) = request.validate() {
		let message = format!("Invalid CLI deployment request: {errors}");
		return json_response(
			StatusCode::BAD_REQUEST,
			ApiErrorResponse { error: &message },
		);
	}

	let organization_id = current_organization_id_for_user(user.id).await?;
	let Some(cluster) = Cluster::objects()
		.filter(Cluster::field_name().eq(request.cluster.clone()))
		.filter(Cluster::field_organization_id().eq(organization_id))
		.first()
		.await
		.map_err(|e| {
			error!(
				"Failed to load cluster {} for CLI deploy in organization {}: {e}",
				request.cluster, organization_id
			);
			AppError::Internal("Failed to load cluster".to_string())
		})?
	else {
		return json_response(
			StatusCode::NOT_FOUND,
			ApiErrorResponse {
				error: "Cluster not found",
			},
		);
	};

	let deployment = match submit_project_deployment(
		&agent_registry.0,
		SubmitProjectDeploymentInput {
			organization_id,
			project_name: &request.project_name,
			cluster: &cluster,
			namespace: Some(&request.namespace),
			image: &request.image,
			project_yaml: &request.project_yaml,
		},
	)
	.await
	{
		Ok(deployment) => deployment,
		Err(err) => return deployment_error_response(&err),
	};

	let deployment_id = deployment
		.id
		.ok_or_else(|| AppError::Internal("Deployment row missing primary key".to_string()))?;
	json_response(
		StatusCode::ACCEPTED,
		CliDeploymentResponse {
			deployment_id,
			project_name: deployment.project_name,
			cluster: cluster.name,
			status: deployment.status,
			image: deployment.image,
		},
	)
}

fn trim_request(request: &mut CliDeploymentRequest) {
	request.project_name = request.project_name.trim().to_string();
	request.cluster = request.cluster.trim().to_string();
	request.namespace = request.namespace.trim().to_string();
	request.image = request.image.trim().to_string();
}

fn deployment_error_response(error: &SubmitProjectDeploymentError) -> ViewResult<Response> {
	let (status, message) = match error {
		SubmitProjectDeploymentError::BadRequest(message) => {
			(StatusCode::BAD_REQUEST, message.as_str())
		}
		SubmitProjectDeploymentError::Conflict(message) => (StatusCode::CONFLICT, message.as_str()),
		SubmitProjectDeploymentError::AgentUnavailable(message) => {
			error!("CLI deploy agent unavailable: {message}");
			(
				StatusCode::SERVICE_UNAVAILABLE,
				"Deployment agent is currently unavailable",
			)
		}
		SubmitProjectDeploymentError::Internal(message) => {
			error!("CLI deploy internal error: {message}");
			(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
		}
	};
	json_response(status, ApiErrorResponse { error: message })
}

fn json_response<T: Serialize>(status: StatusCode, body: T) -> ViewResult<Response> {
	let bytes = json::to_vec(&body)
		.map_err(|e| AppError::Internal(format!("Failed to serialize API response: {e}")))?;
	Ok(Response::new(status)
		.with_header("Content-Type", "application/json")
		.with_body(bytes))
}

#[cfg(test)]
mod tests {
	use reinhardt::StatusCode;
	use rstest::rstest;
	use serde_json::Value;

	use super::{SubmitProjectDeploymentError, deployment_error_response};

	fn error_response_body(error: SubmitProjectDeploymentError) -> (StatusCode, Value) {
		let response = deployment_error_response(&error).expect("error response should render");
		let body = serde_json::from_slice(&response.body).expect("response body should be JSON");
		(response.status, body)
	}

	#[rstest]
	#[case(
		SubmitProjectDeploymentError::BadRequest("Project YAML is required".to_string()),
		StatusCode::BAD_REQUEST,
		"Project YAML is required"
	)]
	#[case(
		SubmitProjectDeploymentError::Conflict("Cluster prod is inactive".to_string()),
		StatusCode::CONFLICT,
		"Cluster prod is inactive"
	)]
	fn test_deployment_error_response_exposes_user_facing_errors(
		#[case] error: SubmitProjectDeploymentError,
		#[case] expected_status: StatusCode,
		#[case] expected_message: &str,
	) {
		// Arrange
		let input = error;

		// Act
		let (status, body) = error_response_body(input);

		// Assert
		assert_eq!(status, expected_status);
		assert_eq!(body["error"], expected_message);
	}

	#[rstest]
	#[case(
		SubmitProjectDeploymentError::AgentUnavailable(
			"Failed to enqueue Project apply command: backend detail".to_string()
		),
		StatusCode::SERVICE_UNAVAILABLE,
		"Deployment agent is currently unavailable"
	)]
	#[case(
		SubmitProjectDeploymentError::Internal("database password leaked".to_string()),
		StatusCode::INTERNAL_SERVER_ERROR,
		"Internal server error"
	)]
	fn test_deployment_error_response_hides_internal_errors(
		#[case] error: SubmitProjectDeploymentError,
		#[case] expected_status: StatusCode,
		#[case] expected_message: &str,
	) {
		// Arrange
		let raw_message = error.message().to_string();

		// Act
		let (status, body) = error_response_body(error);

		// Assert
		assert_eq!(status, expected_status);
		assert_eq!(body["error"], expected_message);
		assert_ne!(body["error"], raw_message);
	}
}
