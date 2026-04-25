//! Repository for the `OrganizationMembership` model.
//!
//! Injectable via DI (`Request` scope). Sub-issue #417 will use this from
//! the `OrgContext` factory. For #415 the repo provides a stable API but
//! is not yet consumed.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::db::orm::{Filter, FilterOperator, FilterValue};
use reinhardt::di::injectable;
use uuid::Uuid;

use crate::apps::organizations::models::OrganizationMembership;
use crate::apps::organizations::roles::MembershipRole;

#[allow(dead_code)] // Consumed by sub-issue #417 (Guard<HasOrgRole<R>>)
#[injectable(scope = Request)]
pub struct OrganizationMembershipRepository;

#[allow(dead_code)] // Consumed by sub-issue #417
impl OrganizationMembershipRepository {
	pub async fn find_by_user_and_org(
		&self,
		user_id: Uuid,
		organization_id: i64,
	) -> Result<Option<OrganizationMembership>, AppError> {
		OrganizationMembership::objects()
			.filter(
				OrganizationMembership::field_user_id(),
				FilterOperator::Eq,
				FilterValue::String(user_id.to_string()),
			)
			.filter(Filter::new(
				OrganizationMembership::field_organization_id(),
				FilterOperator::Eq,
				FilterValue::Integer(organization_id),
			))
			.first()
			.await
			.map_err(|e| AppError::Internal(format!("membership lookup failed: {e}")))
	}

	pub async fn list_for_user(
		&self,
		user_id: Uuid,
	) -> Result<Vec<OrganizationMembership>, AppError> {
		OrganizationMembership::objects()
			.filter(
				OrganizationMembership::field_user_id(),
				FilterOperator::Eq,
				FilterValue::String(user_id.to_string()),
			)
			.order_by(&["created_at"])
			.all()
			.await
			.map_err(|e| AppError::Internal(format!("membership list failed: {e}")))
	}

	pub async fn create(
		&self,
		organization_id: i64,
		user_id: Uuid,
		role: MembershipRole,
	) -> Result<OrganizationMembership, AppError> {
		let new_m = OrganizationMembership {
			id: None,
			organization_id,
			user_id,
			role: role.as_db_str().to_string(),
			created_at: chrono::Utc::now(),
		};
		OrganizationMembership::objects()
			.create(&new_m)
			.await
			.map_err(|e| AppError::Internal(format!("membership create failed: {e}")))
	}
}
