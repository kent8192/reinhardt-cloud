//! Response serializers for cluster endpoints.

use reinhardt::{Schema, ToSchema};
use serde::Serialize;

use crate::apps::clusters::models::Cluster;

/// Cluster API response.
#[derive(Debug, Serialize, Schema)]
pub struct ClusterResponse {
	pub id: Option<i64>,
	pub name: String,
	pub api_url: String,
	pub is_active: bool,
}

impl From<Cluster> for ClusterResponse {
	fn from(c: Cluster) -> Self {
		Self {
			id: c.id,
			name: c.name,
			api_url: c.api_url,
			is_active: c.is_active,
		}
	}
}
