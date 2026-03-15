//! List clusters view.

use reinhardt::core::serde::json;
use reinhardt::http::ViewResult;
use reinhardt::Model;
use reinhardt::{Response, StatusCode, get};

use crate::apps::clusters::models::Cluster;
use crate::apps::clusters::serializers::ClusterResponse;

/// List all clusters.
#[get("/clusters/", name = "cluster_list")]
pub async fn list_clusters() -> ViewResult<Response> {
	let manager = Cluster::objects();
	let clusters = manager.all().all().await.map_err(|e| format!("{e}"))?;
	let responses: Vec<ClusterResponse> = clusters.into_iter().map(ClusterResponse::from).collect();
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&responses)?))
}
