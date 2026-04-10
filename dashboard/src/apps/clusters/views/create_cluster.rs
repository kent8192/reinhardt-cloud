//! Create cluster view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Json, Response, StatusCode, post};
use tracing::error;
use uuid::Uuid;

use crate::apps::clusters::models::Cluster;
use crate::apps::clusters::serializers::{ClusterResponse, CreateClusterRequest};

/// Create a new cluster (authentication required).
///
/// Sets the cluster owner to the authenticated user.
#[post("/", name = "cluster_create")]
pub async fn create_cluster(
	Json(body): Json<CreateClusterRequest>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;
	let new_cluster = Cluster::new(user_id, body.name.clone(), body.api_url.clone(), true);
	let manager = Cluster::objects();
	let created = manager.create(&new_cluster).await.map_err(|e| {
		error!("Failed to create cluster: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;
	let resp = ClusterResponse::from(created);
	Ok(Response::new(StatusCode::CREATED)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
