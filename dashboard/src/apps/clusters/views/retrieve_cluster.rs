//! Retrieve cluster view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{Filter, FilterOperator, FilterValue};
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Path, Response, StatusCode, get};
use tracing::error;
use uuid::Uuid;

use crate::apps::clusters::models::Cluster;
use crate::apps::clusters::serializers::ClusterResponse;

/// Retrieve a single cluster by ID (authentication required).
///
/// Returns 404 if the cluster does not exist or belongs to another user.
#[get("/{id}/", name = "cluster_retrieve")]
pub async fn retrieve_cluster(
	Path(id): Path<i64>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;

	let cluster = Cluster::objects()
		.filter(
			Cluster::field_user_id(),
			FilterOperator::Eq,
			FilterValue::String(user_id.to_string()),
		)
		.filter(Filter::new(
			Cluster::field_id(),
			FilterOperator::Eq,
			FilterValue::Integer(id),
		))
		.first()
		.await
		.map_err(|e| {
			error!("Failed to retrieve cluster: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::NotFound(format!("Cluster with id {id} not found")))?;

	let resp = ClusterResponse::from(cluster);
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
