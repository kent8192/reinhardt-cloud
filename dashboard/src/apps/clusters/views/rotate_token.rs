//! Rotate cluster agent token view.
//!
//! Mints a fresh agent JWT for an existing cluster, returns it once,
//! and replaces the stored Argon2id hash. The old token stops being
//! valid against the registry as soon as the new hash is persisted.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{Filter, FilterOperator, FilterValue};
use reinhardt::di::Depends;
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Path, Response, StatusCode, post};
use tracing::error;
use uuid::Uuid;

use crate::apps::clusters::models::Cluster;
use crate::apps::clusters::serializers::RotateTokenResponse;
use crate::apps::clusters::services::token_issuance::AgentTokenService;
use crate::apps::clusters::views::create_cluster::cluster_id_from_pk;
use crate::apps::organizations::permissions::{Action, require_permission_for_org};

/// Rotate the agent JWT for an existing cluster (authentication required).
///
/// Token rotation is a write-class operation, so we require
/// `Action::ClusterUpdate` (Developer or higher); Viewers receive 403.
/// Returns the new plaintext JWT exactly once. Old tokens are rejected
/// on next verify because the stored hash has changed.
#[post("/orgs/{org}/clusters/{cluster_id}/rotate-token/", name = "rotate_token")]
pub async fn rotate_token(
	Path((org, cluster_id)): Path<(String, i64)>,
	#[inject] AuthInfo(state): AuthInfo,
	#[inject] agent_token_service: Depends<AgentTokenService>,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;
	let organization_id =
		require_permission_for_org(user_id, &org, Action::ClusterUpdate).await?;

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
			error!("Failed to retrieve cluster for token rotation: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::NotFound(format!("Cluster with id {cluster_id} not found")))?;

	let cluster_uuid = cluster_id_from_pk(cluster.id)?;
	let issued = agent_token_service.issue(cluster_uuid)?;
	let now = chrono::Utc::now();

	cluster.token_hash = Some(issued.hash);
	cluster.token_last_rotated_at = Some(now);
	let updated = manager.update(&cluster).await.map_err(|e| {
		error!("Failed to persist rotated token hash: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	let resp = RotateTokenResponse {
		id: updated.id,
		name: updated.name,
		auth_token: issued.plaintext,
		rotated_at: now,
	};
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
