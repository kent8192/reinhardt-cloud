//! Shared deployment submission service.

use std::fmt;
use std::sync::Arc;

use reinhardt::Model;
use reinhardt_cloud_types::crd::Project;
use tracing::error;

use crate::apps::clusters::models::Cluster;
use crate::apps::deployments::models::Deployment;
use crate::apps::deployments::services::agent::{
	send_project_apply_to_cluster, validate_cluster_for_apply,
};
use crate::apps::deployments::services::manifest::validate_project_manifest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitProjectDeploymentError {
	BadRequest(String),
	Conflict(String),
	AgentUnavailable(String),
	Internal(String),
}

impl SubmitProjectDeploymentError {
	pub fn status_code(&self) -> u16 {
		match self {
			Self::BadRequest(_) => 400,
			Self::Conflict(_) => 409,
			Self::AgentUnavailable(_) => 503,
			Self::Internal(_) => 500,
		}
	}

	pub fn message(&self) -> &str {
		match self {
			Self::BadRequest(message)
			| Self::Conflict(message)
			| Self::AgentUnavailable(message)
			| Self::Internal(message) => message,
		}
	}
}

impl fmt::Display for SubmitProjectDeploymentError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(self.message())
	}
}

impl std::error::Error for SubmitProjectDeploymentError {}

#[derive(Clone, Copy)]
pub struct SubmitProjectDeploymentInput<'a> {
	pub organization_id: i64,
	pub project_name: &'a str,
	pub cluster: &'a Cluster,
	pub namespace: Option<&'a str>,
	pub image: &'a str,
	pub project_yaml: &'a str,
}

pub async fn submit_project_deployment(
	registry: &Arc<reinhardt_cloud_grpc::registry::AgentRegistry>,
	input: SubmitProjectDeploymentInput<'_>,
) -> Result<Deployment, SubmitProjectDeploymentError> {
	let project_name = validate_project_name(input.project_name)?;
	let image = validate_image(input.image)?;
	validate_submission_manifest(input)?;
	validate_submission_cluster(input.cluster)?;
	let cluster_id = input.cluster.id.ok_or_else(|| {
		SubmitProjectDeploymentError::Internal("Cluster row missing primary key".to_string())
	})?;

	let deployment = Deployment::build()
		.organization(input.organization_id)
		.project_name(project_name.clone())
		.cluster(cluster_id)
		.status("pending".to_string())
		.image(image)
		.project_yaml(Some(input.project_yaml.to_string()))
		.finish();
	let mut deployment = Deployment::objects()
		.create(&deployment)
		.await
		.map_err(|e| {
			SubmitProjectDeploymentError::Internal(format!("Failed to create deployment: {e}"))
		})?;

	if let Err(e) = send_project_apply_to_cluster(
		registry,
		input.cluster,
		&project_name,
		deployment_yaml(&deployment)?,
	)
	.await
	{
		let deployment_id = deployment.id;
		deployment.status = "error".to_string();
		if let Err(update_err) = Deployment::objects().update(&deployment).await {
			error!(
				"Failed to mark deployment {:?} error after Project apply enqueue failure: {update_err}",
				deployment_id
			);
		}
		return Err(SubmitProjectDeploymentError::AgentUnavailable(format!(
			"Failed to enqueue Project apply command: {e}"
		)));
	}

	Ok(deployment)
}

pub fn validate_submission_cluster(cluster: &Cluster) -> Result<(), SubmitProjectDeploymentError> {
	validate_cluster_for_apply(cluster).map_err(SubmitProjectDeploymentError::Conflict)
}

pub fn validate_submission_manifest(
	input: SubmitProjectDeploymentInput<'_>,
) -> Result<Project, SubmitProjectDeploymentError> {
	if input.project_yaml.trim().is_empty() {
		return Err(SubmitProjectDeploymentError::BadRequest(
			"Project YAML is required".to_string(),
		));
	}
	let project = validate_project_manifest(input.project_yaml)
		.map_err(SubmitProjectDeploymentError::BadRequest)?
		.ok_or_else(|| {
			SubmitProjectDeploymentError::BadRequest("Project YAML is required".to_string())
		})?;
	let manifest_name = project
		.metadata
		.name
		.as_deref()
		.map(str::trim)
		.filter(|value| !value.is_empty())
		.ok_or_else(|| {
			SubmitProjectDeploymentError::BadRequest(
				"Project YAML metadata.name is required".to_string(),
			)
		})?;
	if manifest_name != input.project_name {
		return Err(SubmitProjectDeploymentError::BadRequest(
			"Project YAML metadata.name must match project_name".to_string(),
		));
	}
	if let Some(expected_namespace) = input
		.namespace
		.map(str::trim)
		.filter(|value| !value.is_empty())
	{
		let manifest_namespace = project
			.metadata
			.namespace
			.as_deref()
			.map(str::trim)
			.filter(|value| !value.is_empty())
			.unwrap_or("default");
		if manifest_namespace != expected_namespace {
			return Err(SubmitProjectDeploymentError::BadRequest(
				"Project YAML metadata.namespace must match namespace".to_string(),
			));
		}
	}
	if project.spec.image.trim() != input.image.trim() {
		return Err(SubmitProjectDeploymentError::BadRequest(
			"Project YAML spec.image must match image".to_string(),
		));
	}
	Ok(project)
}

fn validate_project_name(project_name: &str) -> Result<String, SubmitProjectDeploymentError> {
	let project_name = project_name.trim();
	if project_name.is_empty() || project_name.len() > 63 {
		return Err(SubmitProjectDeploymentError::BadRequest(
			"Project name must be 1-63 characters".to_string(),
		));
	}
	Ok(project_name.to_string())
}

fn validate_image(image: &str) -> Result<String, SubmitProjectDeploymentError> {
	let image = image.trim();
	if image.is_empty() || image.len() > 512 {
		return Err(SubmitProjectDeploymentError::BadRequest(
			"Image must be 1-512 characters".to_string(),
		));
	}
	Ok(image.to_string())
}

fn deployment_yaml(deployment: &Deployment) -> Result<&str, SubmitProjectDeploymentError> {
	deployment.project_yaml.as_deref().ok_or_else(|| {
		SubmitProjectDeploymentError::Internal(
			"Deployment row missing Project YAML after insert".to_string(),
		)
	})
}
