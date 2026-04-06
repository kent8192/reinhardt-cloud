//! Delete cluster view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::db::orm::{Filter, FilterOperator, FilterValue};
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Path, Response, StatusCode, delete};
use tracing::error;
use uuid::Uuid;

use crate::apps::clusters::models::Cluster;

/// Delete a cluster by ID (authentication required).
///
/// Returns 204 No Content on success.
/// Returns 404 if the cluster does not exist or belongs to another user.
#[delete("/clusters/{id}", name = "cluster_delete")]
pub async fn delete_cluster(
	Path(id): Path<i64>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;

	// Verify ownership before deleting
	let cluster = Cluster::objects()
		.filter(
			Cluster::field_user_id(),
			FilterOperator::Eq,
			FilterValue::String(user_id.to_string()),
		)
		.filter(Filter::new(
			"id",
			FilterOperator::Eq,
			FilterValue::Integer(id),
		))
		.first()
		.await
		.map_err(|e| {
			error!("Failed to retrieve cluster for deletion: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::NotFound(format!("Cluster with id {id} not found")))?;

	let cluster_id = cluster
		.id
		.ok_or_else(|| AppError::Internal("Cluster has no ID".to_string()))?;

	Cluster::objects().delete(cluster_id).await.map_err(|e| {
		error!("Failed to delete cluster: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	Ok(Response::new(StatusCode::NO_CONTENT))
}
