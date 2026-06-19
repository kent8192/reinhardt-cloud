//! CLI deployment endpoint serializers.

use reinhardt::{Schema, ToSchema, Validate};
use serde::{Deserialize, Serialize};

/// Request body submitted by `reinhardt-cloud deploy`.
#[derive(Debug, Clone, Deserialize, Validate, Schema)]
pub struct CliDeploymentRequest {
	#[validate(length(min = 1, max = 63))]
	pub project_name: String,
	#[validate(length(min = 1, max = 63))]
	pub cluster: String,
	#[validate(length(min = 1, max = 63))]
	pub namespace: String,
	#[validate(length(min = 1, max = 512))]
	pub image: String,
	#[validate(length(min = 1, max = 65535))]
	pub project_yaml: String,
}

/// Deployment submission response returned to the CLI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CliDeploymentResponse {
	pub deployment_id: i64,
	pub project_name: String,
	pub cluster: String,
	pub status: String,
	pub image: String,
}
