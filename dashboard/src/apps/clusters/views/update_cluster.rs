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
use crate::apps::organizations::permissions::{Action, require_permission_for_org};

/// Update an existing cluster (authentication required).
///
/// Requires `Action::ClusterUpdate` (Developer or higher); Viewers receive 403.
/// Supports partial updates: only provided fields are modified.
/// Returns 404 if the cluster does not exist or does not belong to the
/// specified organization.
#[patch("/orgs/{org}/clusters/{cluster_id}/", name = "update")]
pub async fn update_cluster(
	Path((cluster_id, org)): Path<(i64, String)>,
	Json(body): Json<UpdateClusterRequest>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;
	let organization_id =
		require_permission_for_org(user_id, &org, Action::ClusterUpdate).await?;

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
			FilterValue::Integer(cluster_id),
		))
		.first()
		.await
		.map_err(|e| {
			error!("Failed to retrieve cluster for update: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::NotFound(format!("Cluster with id {cluster_id} not found")))?;

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

	let updated = match manager.update(&cluster).await {
		Ok(c) => c,
		Err(e) => {
			// Detect database UNIQUE constraint violation on
			// `(organization_id, name)`. The ORM does not expose a
			// structured variant for this case, so we string-match
			// (mirrors the pattern used by `apps/auth/views/register.rs`).
			let err_lower = e.to_string().to_lowercase();
			if err_lower.contains("unique") || err_lower.contains("duplicate") {
				return Err(AppError::Conflict(
					"Cluster name already exists in this organization".to_string(),
				));
			}
			error!("Failed to update cluster: {e}");
			return Err(AppError::Internal("Internal server error".to_string()));
		}
	};

	let resp = ClusterResponse::from(updated);
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
