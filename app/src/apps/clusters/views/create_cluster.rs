//! Create cluster view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::http::{AuthState, ViewResult};
use reinhardt::{Request, Response, StatusCode, post};
use uuid::Uuid;

use crate::apps::clusters::models::Cluster;
use crate::apps::clusters::serializers::{ClusterResponse, CreateClusterRequest};

/// Create a new cluster (authentication required).
///
/// Sets the cluster owner to the authenticated user.
///
/// Workaround: Uses `AuthState::from_extensions` instead of `CurrentUser<User>`
/// DI injection because `CurrentUser` DB lookup requires complex DI configuration
/// that is not yet fully supported in the reinhardt-web test environment.
/// See: <https://github.com/kent8192/reinhardt-web/issues/2419>
#[post("/clusters/", name = "cluster_create")]
pub async fn create_cluster(request: Request) -> ViewResult<Response> {
	let auth_state = AuthState::from_extensions(&request.extensions)
		.filter(|s| s.is_authenticated())
		.ok_or_else(|| AppError::Authentication("Authentication required".to_string()))?;
	let user_id = Uuid::parse_str(auth_state.user_id())
		.map_err(|e| AppError::Internal(format!("Invalid user ID in token: {e}")))?;

	let body: CreateClusterRequest = request.json()?;
	let new_cluster = Cluster::new(user_id, body.name.clone(), body.api_url.clone(), true);
	let manager = Cluster::objects();
	let created = manager
		.create(&new_cluster)
		.await
		.map_err(|e| format!("{e}"))?;
	let resp = ClusterResponse::from(created);
	Ok(Response::new(StatusCode::CREATED)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
