//! Create cluster view.

use reinhardt::core::serde::json;
use reinhardt::http::ViewResult;
use reinhardt::Model;
use reinhardt::{Json, Response, StatusCode, post};

use crate::apps::clusters::models::Cluster;
use crate::apps::clusters::serializers::{ClusterResponse, CreateClusterRequest};

/// Create a new cluster.
#[post("/clusters/", name = "cluster_create", pre_validate = true)]
pub async fn create_cluster(body: Json<CreateClusterRequest>) -> ViewResult<Response> {
	let new_cluster = Cluster::new(body.name.clone(), body.api_url.clone(), true);
	let manager = Cluster::objects();
	let created = manager.create(&new_cluster).await.map_err(|e| format!("{e}"))?;
	let resp = ClusterResponse::from(created);
	Ok(Response::new(StatusCode::CREATED)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
