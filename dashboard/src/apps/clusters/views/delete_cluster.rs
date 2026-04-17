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
#[delete("/{id}/", name = "delete")]
pub async fn delete_cluster(
	Path(id): Path<i64>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;

	// Verify ownership before deleting
	Cluster::objects()
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
			error!("Failed to retrieve cluster for deletion: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::NotFound(format!("Cluster with id {id} not found")))?;

	// Use path id directly for deletion — the ownership check above
	// already confirmed the record exists and belongs to this user
	Cluster::objects().delete(id).await.map_err(|e| {
		let err_msg = e.to_string();
		// Detect foreign key constraint violations (e.g., deployments referencing this cluster)
		if err_msg.contains("foreign key")
			|| err_msg.contains("FOREIGN KEY")
			|| err_msg.contains("RESTRICT")
		{
			return AppError::Conflict(
				"Cannot delete cluster: it still has associated deployments".to_string(),
			);
		}
		error!("Failed to delete cluster: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	Ok(Response::new(StatusCode::NO_CONTENT))
}
