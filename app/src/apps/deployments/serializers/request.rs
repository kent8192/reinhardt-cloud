//! Request serializers for deployment endpoints.

use serde::Deserialize;

/// Request body for creating a deployment.
#[derive(Debug, Deserialize)]
pub struct CreateDeploymentRequest {
	pub app_name: String,
	pub cluster_id: i64,
	pub image: String,
}
