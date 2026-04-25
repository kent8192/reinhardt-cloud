//! Repository for the `Organization` model.
//!
//! Injectable via DI (`Request` scope). Sub-issue #417 will use this from
//! the `OrgContext` factory. For #415 the repo provides a stable API but
//! is not yet consumed (existing dashboard views use ORM static methods
//! directly, matching the `User` model pattern).

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::db::orm::{FilterOperator, FilterValue};
use reinhardt::di::injectable;

use crate::apps::organizations::models::Organization;

#[allow(dead_code)] // Consumed by sub-issue #417 (Guard<HasOrgRole<R>>)
#[injectable(scope = Request)]
pub struct OrganizationRepository;

#[allow(dead_code)] // Consumed by sub-issue #417
impl OrganizationRepository {
	pub async fn find_by_slug(&self, slug: &str) -> Result<Option<Organization>, AppError> {
		Organization::objects()
			.filter(
				Organization::field_slug(),
				FilterOperator::Eq,
				FilterValue::String(slug.to_string()),
			)
			.first()
			.await
			.map_err(|e| AppError::Internal(format!("organization lookup failed: {e}")))
	}

	pub async fn find_by_id(&self, id: i64) -> Result<Option<Organization>, AppError> {
		Organization::objects()
			.filter(
				Organization::field_id(),
				FilterOperator::Eq,
				FilterValue::Integer(id),
			)
			.first()
			.await
			.map_err(|e| AppError::Internal(format!("organization lookup failed: {e}")))
	}

	pub async fn create(&self, slug: &str, name: &str) -> Result<Organization, AppError> {
		let now = chrono::Utc::now();
		let new_org = Organization {
			id: None,
			slug: slug.to_string(),
			name: name.to_string(),
			created_at: now,
			updated_at: now,
		};
		Organization::objects()
			.create(&new_org)
			.await
			.map_err(|e| AppError::Internal(format!("organization create failed: {e}")))
	}
}
