//! Request serializers for deployment endpoints.

use reinhardt::{Schema, ToSchema, Validate};
use serde::Deserialize;

/// Request body for creating a deployment.
#[derive(Debug, Clone, Deserialize, Validate, Schema)]
pub struct CreateDeploymentRequest {
	#[validate(length(min = 1, max = 63))]
	pub app_name: String,
	#[validate(range(min = 1))]
	pub cluster_id: i64,
	#[validate(length(min = 1, max = 512))]
	pub image: String,
}

/// Request body for updating a deployment (all fields optional).
#[derive(Debug, Clone, Deserialize, Validate, Schema)]
pub struct UpdateDeploymentRequest {
	#[validate(length(min = 1, max = 63))]
	pub app_name: Option<String>,
	#[validate(length(min = 1, max = 512))]
	pub image: Option<String>,
	#[validate(length(min = 1, max = 50))]
	pub status: Option<String>,
}

/// Request body for updating deployment status from an agent.
#[derive(Debug, Clone, Deserialize, Validate, Schema)]
pub struct DeploymentStatusRequest {
	#[validate(length(min = 1, max = 50))]
	pub status: String,
}
