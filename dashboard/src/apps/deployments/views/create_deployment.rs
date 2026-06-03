//! Create deployment view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Json, Path, Response, StatusCode, post};
use reinhardt_cloud_types::crd::ReinhardtApp;
use tracing::error;
use uuid::Uuid;

use crate::apps::clusters::models::Cluster;
use crate::apps::deployments::models::Deployment;
use crate::apps::deployments::serializers::{CreateDeploymentRequest, DeploymentResponse};
use crate::apps::organizations::permissions::{Action, require_permission_for_org};

fn validate_reinhardt_app_yaml(manifest: &str) -> Result<(), AppError> {
	let app: ReinhardtApp = serde_yaml::from_str(manifest)
		.map_err(|e| AppError::Validation(format!("Invalid ReinhardtApp YAML: {e}")))?;
	if let Err(errors) = app.spec.validate() {
		let messages = errors
			.into_iter()
			.map(|e| e.message)
			.collect::<Vec<_>>()
			.join("; ");
		return Err(AppError::Validation(format!(
			"Invalid ReinhardtApp spec: {messages}"
		)));
	}

	Ok(())
}

/// Create a new deployment (authentication required).
///
/// Requires `Action::DeploymentCreate` (Developer or higher); Viewers
/// receive 403. Sets the deployment owner to the specified organization.
/// Validates that the target cluster belongs to the same organization.
#[post("/orgs/{org}/deployments/", name = "create")]
pub async fn create_deployment(
	Path(org_slug): Path<String>,
	Json(body): Json<CreateDeploymentRequest>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;
	let organization_id =
		require_permission_for_org(user_id, &org_slug, Action::DeploymentCreate).await?;

	// Validate cluster exists and belongs to the specified organization.
	let cluster_exists = Cluster::objects()
		.filter(Cluster::field_id().eq(body.cluster_id))
		.filter(Cluster::field_organization_id().eq(organization_id))
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

	if let Some(manifest) = body.reinhardt_app_yaml.as_deref() {
		validate_reinhardt_app_yaml(manifest)?;
	}

	let new_deployment = Deployment::build()
		.organization_id(organization_id)
		.app_name(body.app_name.clone())
		.cluster_id(body.cluster_id)
		.status("pending".to_string())
		.image(body.image.clone())
		.reinhardt_app_yaml(body.reinhardt_app_yaml.clone())
		.finish();
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

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_validate_reinhardt_app_yaml_accepts_valid_manifest() {
		// Arrange
		let manifest = r#"
apiVersion: paas.reinhardt-cloud.dev/v1alpha2
kind: ReinhardtApp
metadata:
  name: web
  namespace: default
spec:
  image: ghcr.io/example/web:v1
  health:
    path: /healthz
    port: 8000
    interval_seconds: 10
"#;

		// Act
		let result = validate_reinhardt_app_yaml(manifest);

		// Assert
		assert!(result.is_ok());
	}

	#[rstest]
	fn test_validate_reinhardt_app_yaml_rejects_invalid_manifest() {
		// Arrange
		let manifest = "not: [valid";

		// Act
		let result = validate_reinhardt_app_yaml(manifest);

		// Assert
		assert!(
			matches!(result, Err(AppError::Validation(_))),
			"expected validation error, got {result:?}"
		);
	}

	#[rstest]
	fn test_validate_reinhardt_app_yaml_rejects_invalid_spec() {
		// Arrange
		let manifest = r#"
apiVersion: paas.reinhardt-cloud.dev/v1alpha2
kind: ReinhardtApp
metadata:
  name: web
  namespace: default
spec:
  image: ghcr.io/example/web:v1
  health:
    port: 0
"#;

		// Act
		let result = validate_reinhardt_app_yaml(manifest);

		// Assert
		assert!(
			matches!(result, Err(AppError::Validation(_))),
			"expected validation error, got {result:?}"
		);
	}
}
