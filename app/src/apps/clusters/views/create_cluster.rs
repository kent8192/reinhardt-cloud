//! Create cluster view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Json, Response, StatusCode, post};
use uuid::Uuid;

use crate::apps::clusters::models::Cluster;
use crate::apps::clusters::serializers::{ClusterResponse, CreateClusterRequest};

/// Create a new cluster (authentication required).
///
/// Sets the cluster owner to the authenticated user.
#[post("/clusters/", name = "cluster_create")]
pub async fn create_cluster(
	body: Json<CreateClusterRequest>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Internal(format!("Invalid user ID in token: {e}")))?;

	let body = body.0;
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
