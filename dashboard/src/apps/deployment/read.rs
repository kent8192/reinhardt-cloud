//! Bounded operation projections for authorized project reads.

use reinhardt::db::orm::Model;
use uuid::Uuid;

use super::models::Operation;
use super::services::OperationSnapshot;

/// Project projections call this after checking current organization membership.
pub(crate) async fn latest_for_environment(
	organization: Uuid,
	environment: Uuid,
) -> reinhardt::core::exception::Result<Option<OperationSnapshot>> {
	let operations = Operation::objects()
		.filter(Operation::field_organization_id().eq(organization))
		.filter(Operation::field_environment_id().eq(environment))
		.order_by(&["-created_at", "-id"])
		.limit(1)
		.all()
		.await?;
	operations
		.first()
		.map(|operation| {
			Ok(OperationSnapshot {
				progress: super::services::OperationProgress {
					id: operation.id,
					environment_id: operation.environment_id(),
					environment_version: operation.environment_version,
					state: super::services::OperationState::from_storage(&operation.state)
						.ok_or_else(|| {
							reinhardt::core::exception::Error::Internal(
								"Unsupported operation state".into(),
							)
						})?,
				},
				kind: operation.kind.clone(),
				created_at: operation.created_at,
			})
		})
		.transpose()
}
