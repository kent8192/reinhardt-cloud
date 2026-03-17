//! Request serializers for deployment endpoints.

use reinhardt::{Schema, ToSchema, Validate};
use serde::Deserialize;

/// Request body for creating a deployment.
#[derive(Debug, Clone, Deserialize, Validate, Schema)]
pub struct CreateDeploymentRequest {
	#[validate(length(min = 1, max = 63))]
	pub app_name: String,
	pub cluster_id: i64,
	#[validate(length(min = 1))]
	pub image: String,
}
