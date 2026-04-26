//! Update cluster view.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{Filter, FilterOperator, FilterValue};
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Json, Path, Response, StatusCode, Validate, patch};
use tracing::error;
use uuid::Uuid;

use crate::apps::clusters::models::Cluster;
use crate::apps::clusters::serializers::{ClusterResponse, UpdateClusterRequest};
use crate::apps::organizations::helpers::current_organization_id_for_user;

/// Update an existing cluster (authentication required).
///
/// Supports partial updates: only provided fields are modified.
/// Returns 404 if the cluster does not exist or does not belong to the
/// authenticated user's active organization.
#[patch("/{id}/", name = "update")]
pub async fn update_cluster(
	Path(id): Path<i64>,
	Json(body): Json<UpdateClusterRequest>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;
	let organization_id = current_organization_id_for_user(user_id).await?;

	// Validate the request body
	body.validate()?;

	// Reject empty updates -- at least one field must be provided
	if body.name.is_none() && body.api_url.is_none() && body.is_active.is_none() {
		return Err(AppError::Validation(
			"At least one field must be provided for update".to_string(),
		));
	}

	let manager = Cluster::objects();
	let mut cluster = manager
		.filter(
			Cluster::field_organization_id(),
			FilterOperator::Eq,
			FilterValue::Integer(organization_id),
		)
		.filter(Filter::new(
			Cluster::field_id(),
			FilterOperator::Eq,
			FilterValue::Integer(id),
		))
		.first()
		.await
		.map_err(|e| {
			error!("Failed to retrieve cluster for update: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::NotFound(format!("Cluster with id {id} not found")))?;

	// Apply partial updates
	if let Some(name) = body.name {
		cluster.name = name;
	}
	if let Some(api_url) = body.api_url {
		cluster.api_url = api_url;
	}
	if let Some(is_active) = body.is_active {
		cluster.is_active = is_active;
	}

	let updated = manager.update(&cluster).await.map_err(|e| {
		error!("Failed to update cluster: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	let resp = ClusterResponse::from(updated);
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
