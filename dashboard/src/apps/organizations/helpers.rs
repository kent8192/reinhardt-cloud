//! Interim helpers for the organizations app.
//!
//! `current_organization_id_for_user` resolves the user's "active
//! organization" by looking up their first `OrganizationMembership` ordered
//! by creation time. The function is wrapped by
//! [`crate::apps::organizations::permissions::require_permission`]
//! (introduced by issue #417), which adds RBAC checks on top of the
//! organization lookup. View handlers should call `require_permission`
//! and not this helper directly. A future URL reshape (sub-issue #418)
//! will replace the "active organization" notion with an explicit
//! `{org_slug}` URL parameter.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::db::orm::{FilterOperator, FilterValue};
use uuid::Uuid;

use crate::apps::organizations::models::OrganizationMembership;

/// Returns the `organization_id` of the first membership the user holds,
/// ordered by `created_at` ascending. This is the user's Personal Org for
/// users registered after #415.
///
/// Returns `AppError::NotFound` if the user has no memberships. This should
/// be impossible for users registered after #415 (the registration view
/// always provisions a Personal Org); a stranded account indicates either
/// a partial-failure rollback bug or a pre-#415 dev account that needs
/// re-registration.
pub async fn current_organization_id_for_user(user_id: Uuid) -> Result<i64, AppError> {
	let m = OrganizationMembership::objects()
		.filter(
			OrganizationMembership::field_user_id(),
			FilterOperator::Eq,
			FilterValue::String(user_id.to_string()),
		)
		.order_by(&["created_at"])
		.first()
		.await
		.map_err(|e| AppError::Internal(format!("membership lookup failed: {e}")))?
		.ok_or_else(|| {
			AppError::NotFound(
				"user has no organization membership; re-register to provision one".to_string(),
			)
		})?;
	Ok(m.organization_id)
}
