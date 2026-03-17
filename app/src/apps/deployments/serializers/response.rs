//! Response serializers for deployment endpoints.

use reinhardt::{Schema, ToSchema};
use serde::Serialize;

use crate::apps::deployments::models::Deployment;

/// Deployment API response.
#[derive(Debug, Serialize, Schema)]
pub struct DeploymentResponse {
	pub id: i64,
	pub app_name: String,
	pub cluster_id: i64,
	pub status: String,
	pub image: String,
}

impl From<Deployment> for DeploymentResponse {
	fn from(d: Deployment) -> Self {
		Self {
			id: d.id.unwrap_or(0),
			app_name: d.app_name,
			cluster_id: d.cluster_id,
			status: d.status,
			image: d.image,
		}
	}
}
