//! Create cluster view.
//!
//! Upon successful insertion, mints an agent JWT (containing the
//! cluster UUID used as `cluster_id`), returns the plaintext to the
//! caller exactly once, and persists only an Argon2id hash.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::di::Depends;
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Json, Path, Response, StatusCode, post};
use tracing::error;
use uuid::Uuid;

use crate::apps::clusters::models::Cluster;
use crate::apps::clusters::serializers::{CreateClusterRequest, CreateClusterResponse};
use crate::apps::clusters::services::token_issuance::AgentTokenService;
use crate::apps::organizations::permissions::{Action, require_permission_for_org};

/// Create a new cluster (authentication required).
///
/// Requires `Action::ClusterCreate` (Developer or higher); Viewers receive 403.
/// On success, returns the minted agent JWT exactly once -- it is never
/// retrievable afterwards. Rotate via `POST /orgs/{org}/clusters/{id}/rotate-token/`.
#[post("/orgs/{org}/clusters/", name = "create")]
pub async fn create_cluster(
	Path(org_slug): Path<String>,
	Json(body): Json<CreateClusterRequest>,
	#[inject] AuthInfo(state): AuthInfo,
	#[inject] agent_token_service: Depends<AgentTokenService>,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;
	let organization_id =
		require_permission_for_org(user_id, &org_slug, Action::ClusterCreate).await?;

	// Mint the agent token up-front so we never persist a cluster row
	// without the corresponding token hash. The cluster UUID used as
	// `cluster_id` is derived from the DB-generated `id` once known;
	// we therefore first insert the cluster, then mint and update.
	let new_cluster = Cluster::build()
		.organization_id(organization_id)
		.name(body.name.clone())
		.api_url(body.api_url.clone())
		.is_active(true)
		.token_hash(None)
		.token_last_rotated_at(None)
		.finish();
	let manager = Cluster::objects();
	let mut created = match manager.create(&new_cluster).await {
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
			error!("Failed to create cluster: {e}");
			return Err(AppError::Internal("Internal server error".to_string()));
		}
	};

	// Derive a stable cluster_id from the inserted row's primary key.
	let cluster_uuid = cluster_id_from_pk(created.id)?;
	let issued = agent_token_service.issue(cluster_uuid)?;

	// Persist the hash + rotation timestamp.
	created.token_hash = Some(issued.hash);
	created.token_last_rotated_at = Some(chrono::Utc::now());
	let updated = manager.update(&created).await.map_err(|e| {
		error!("Failed to persist agent token hash: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	let resp = CreateClusterResponse {
		id: updated.id,
		name: updated.name,
		api_url: updated.api_url,
		is_active: updated.is_active,
		auth_token: issued.plaintext,
	};
	Ok(Response::new(StatusCode::CREATED)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}

/// Derive a deterministic cluster UUID from the row's primary key.
///
/// We embed the `i64` PK in the low 64 bits of a UUID so that a cluster
/// retains the same `cluster_id` across token rotations. Using UUID v4
/// instead would break the invariant that an agent's JWT permanently
/// identifies its cluster.
pub(crate) fn cluster_id_from_pk(id: Option<i64>) -> Result<Uuid, AppError> {
	let pk = id.ok_or_else(|| {
		AppError::Internal("Cluster row missing primary key after insert".to_string())
	})?;
	let mut bytes = [0u8; 16];
	// First 8 bytes: a fixed namespace marker for reinhardt-cloud cluster IDs.
	bytes[..8].copy_from_slice(b"RHCL-CID");
	bytes[8..].copy_from_slice(&pk.to_be_bytes());
	Ok(Uuid::from_bytes(bytes))
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_cluster_id_from_pk_is_deterministic() {
		// Arrange
		let pk = Some(42i64);

		// Act
		let a = cluster_id_from_pk(pk).unwrap();
		let b = cluster_id_from_pk(pk).unwrap();

		// Assert
		assert_eq!(a, b);
	}

	#[rstest]
	fn test_cluster_id_from_pk_differs_per_id() {
		// Arrange + Act
		let a = cluster_id_from_pk(Some(1)).unwrap();
		let b = cluster_id_from_pk(Some(2)).unwrap();

		// Assert
		assert_ne!(a, b);
	}

	#[rstest]
	fn test_cluster_id_from_pk_rejects_none() {
		// Act
		let result = cluster_id_from_pk(None);

		// Assert
		assert!(result.is_err());
	}
}
