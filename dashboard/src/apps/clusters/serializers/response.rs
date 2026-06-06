//! Response serializers for cluster endpoints.

use reinhardt::{Schema, ToSchema};
use serde::Serialize;

use crate::apps::clusters::models::Cluster;
/// Cluster API response.
///
/// Used for list/retrieve/update endpoints. **Never** carries the agent
/// token — the plaintext token is returned exactly once by
/// `CreateClusterResponse` / `RotateTokenResponse`.
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

/// Response body for cluster creation — returns the agent JWT exactly once.
///
/// The token plaintext is **NOT** retrievable afterwards; persist it
/// client-side or rotate via `POST /clusters/{id}/rotate-token/`.
#[derive(Debug, Serialize, Schema)]
pub struct CreateClusterResponse {
	pub id: Option<i64>,
	pub name: String,
	pub api_url: String,
	pub is_active: bool,
	/// The minted agent JWT. Returned once — never persisted in plaintext.
	pub auth_token: String,
}

/// Response body for token rotation — returns the fresh agent JWT once.
#[derive(Debug, Serialize, Schema)]
pub struct RotateTokenResponse {
	pub id: Option<i64>,
	pub name: String,
	/// The newly-minted agent JWT. Returned once.
	pub auth_token: String,
	/// ISO-8601 timestamp of the rotation.
	pub rotated_at: chrono::DateTime<chrono::Utc>,
}
