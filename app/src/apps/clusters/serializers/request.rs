//! Request serializers for cluster endpoints.

use reinhardt::Validate;
use serde::Deserialize;

/// Request body for creating a cluster.
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreateClusterRequest {
	#[validate(length(min = 1, max = 63))]
	pub name: String,
	#[validate(url)]
	pub api_url: String,
}
