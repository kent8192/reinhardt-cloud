//! Request serializers for cluster endpoints.

use serde::Deserialize;

/// Request body for creating a cluster.
#[derive(Debug, Deserialize)]
pub struct CreateClusterRequest {
	pub name: String,
	pub api_url: String,
}
