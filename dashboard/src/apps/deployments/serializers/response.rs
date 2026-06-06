//! Response serializers for deployment endpoints.

use reinhardt::{Schema, ToSchema};
use serde::Serialize;

use crate::apps::deployments::models::Deployment;
/// Deployment API response.
#[derive(Debug, Serialize, Schema)]
pub struct DeploymentResponse {
	pub id: Option<i64>,
	pub app_name: String,
	pub cluster_id: i64,
	pub status: String,
	pub image: String,
}

impl From<Deployment> for DeploymentResponse {
	fn from(d: Deployment) -> Self {
		Self {
			id: d.id,
			app_name: d.app_name,
			cluster_id: d.cluster_id,
			status: d.status,
			image: d.image,
		}
	}
}

/// Single log entry returned in the logs response.
#[derive(Debug, Serialize, Schema)]
pub struct LogEntry {
	pub timestamp: String,
	pub message: String,
	pub level: String,
}

/// Response body for the deployment logs endpoint.
#[derive(Debug, Serialize, Schema)]
pub struct DeploymentLogsResponse {
	pub logs: Vec<LogEntry>,
}
