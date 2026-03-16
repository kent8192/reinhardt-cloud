//! List clusters view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::http::ViewResult;
use reinhardt::{CurrentUser, Response, StatusCode, get};

use crate::apps::auth::models::User;
use crate::apps::clusters::models::Cluster;
use crate::apps::clusters::serializers::ClusterResponse;

/// List all clusters (authentication required).
#[get("/clusters/", name = "cluster_list", use_inject = true)]
pub async fn list_clusters(#[inject] user: CurrentUser<User>) -> ViewResult<Response> {
	eprintln!(
		"[DEBUG VIEW] CurrentUser: is_authenticated={}",
		user.is_authenticated()
	);
	if !user.is_authenticated() {
		return Err(AppError::Authentication(
			"Authentication required".to_string(),
		));
	}

	let manager = Cluster::objects();
	let clusters = manager.all().all().await.map_err(|e| format!("{e}"))?;
	let responses: Vec<ClusterResponse> = clusters.into_iter().map(ClusterResponse::from).collect();
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&responses)?))
}
