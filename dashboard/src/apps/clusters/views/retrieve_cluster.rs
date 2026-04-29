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
use crate::apps::organizations::permissions::{Action, require_permission_for_org};

/// Retrieve a single cluster by ID, scoped to the specified organization.
///
/// Workaround for kent8192/reinhardt-web#4013 (tracked in reinhardt-cloud#466)
/// Remove this comment when the upstream issue is resolved.
///
/// Ideal implementation (without workaround):
///   `Path((org, cluster_id)): Path<(String, i64)>` — URL pattern order
///
/// `Path<(T1, T2)>` sorts path parameters alphabetically by name before
/// filling the tuple, not in URL pattern order. For this route:
/// "cluster_id" < "org" alphabetically → tuple[0]=cluster_id, tuple[1]=org.
/// Therefore `Path<(i64, String)>` is required (i64=cluster_id, String=org).
///
/// Requires `Action::ClusterRead` (Viewer or higher); returns 403 if the
/// caller's role does not permit the action. Returns 404 if the cluster
/// does not exist or does not belong to the specified org.
#[get("/orgs/{org}/clusters/{cluster_id}/", name = "retrieve")]
pub async fn retrieve_cluster(
	Path((cluster_id, org)): Path<(i64, String)>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;
	let organization_id = require_permission_for_org(user_id, &org, Action::ClusterRead).await?;

	let cluster = Cluster::objects()
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
			error!("Failed to retrieve cluster: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::NotFound(format!("Cluster with id {cluster_id} not found")))?;

	let resp = ClusterResponse::from(cluster);
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
