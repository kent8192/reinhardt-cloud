//! Request serializers for cluster endpoints.

use reinhardt::{Schema, ToSchema, Validate};
use serde::Deserialize;

/// Request body for creating a cluster.
#[derive(Debug, Clone, Deserialize, Validate, Schema)]
pub struct CreateClusterRequest {
	#[validate(length(min = 1, max = 63))]
	pub name: String,
	#[validate(url, length(max = 2048))]
	pub api_url: String,
}

/// Request body for updating a cluster (partial update).
#[derive(Debug, Clone, Deserialize, Validate, Schema)]
pub struct UpdateClusterRequest {
	#[validate(length(min = 1, max = 63))]
	pub name: Option<String>,
	#[validate(url, length(max = 2048))]
	pub api_url: Option<String>,
	pub is_active: Option<bool>,
}
