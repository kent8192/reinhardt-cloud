//! View-side guard helpers for RBAC enforcement.
//!
//! Views call [`require_permission`] at the top of the handler to
//! resolve the user's active organization, look up their membership
//! role, and verify the role is allowed to perform the requested
//! [`Action`]. The function returns the resolved `organization_id` so
//! that the view can immediately use it as the multi-tenant filter on
//! its ORM queries.
//!
//! This replaces the per-view pattern:
//!
//! ```ignore
//! let user_id = ...;
//! let organization_id = current_organization_id_for_user(user_id).await?;
//! // ... no permission check beyond org membership ...
//! ```
//!
//! with:
//!
//! ```ignore
//! let user_id = ...;
//! let organization_id = require_permission(user_id, Action::ClusterDelete).await?;
//! ```

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use reinhardt::db::orm::{Filter, FilterOperator, FilterValue};
use tracing::error;
use uuid::Uuid;

use crate::apps::organizations::models::OrganizationMembership;
use crate::apps::organizations::permissions::action::Action;
use crate::apps::organizations::permissions::table::allowed;
use crate::apps::organizations::roles::MembershipRole;

/// Resolve the role that `user_id` holds inside `organization_id`, or
/// return `Ok(None)` if no membership exists.
///
/// Wraps the `OrganizationMembership` table lookup in a single function so
/// every guard call site shares identical semantics for "no membership".
/// Database errors propagate as 500; an unparseable role string is treated
/// as a 500 because the DB CHECK constraint should prevent unknown values.
pub async fn resolve_membership_role(
	user_id: Uuid,
	organization_id: i64,
) -> Result<Option<MembershipRole>, AppError> {
	let membership = OrganizationMembership::objects()
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
		.map_err(|e| {
			error!("Failed to look up organization membership: {e}");
			AppError::Internal("Internal server error".to_string())
		})?;

	let Some(membership) = membership else {
		return Ok(None);
	};

	let role = MembershipRole::from_db_str(&membership.role).ok_or_else(|| {
		error!(
			"Unknown membership role '{}' in DB; CHECK constraint should prevent this",
			membership.role
		);
		AppError::Internal("Internal server error".to_string())
	})?;
	Ok(Some(role))
}

/// Verify that `user_id` is permitted to perform `action` in their
/// active organization, returning that organization's id on success.
///
/// # Behaviour
///
/// 1. Resolve the user's active organization via
///    `current_organization_id_for_user` (see
///    `crate::apps::organizations::helpers`).
/// 2. Look up the user's role in that organization.
/// 3. Consult the static [`allowed`] matrix.
///
/// # Errors
///
/// - 404 (`AppError::NotFound`) — the user has no organization
///   membership at all (only possible for stranded pre-#415 dev accounts;
///   matches the existing helper's behaviour).
/// - 403 (`AppError::Authorization`) — the user has no membership in the
///   target organization, or their role is not permitted to perform the
///   action.
/// - 500 (`AppError::Internal`) — database or data-integrity failure.
///
/// Used only by the deprecated flat-URL redirect middleware. New code should
/// call `require_permission_for_org` instead.
pub async fn require_permission(user_id: Uuid, action: Action) -> Result<i64, AppError> {
	use crate::apps::organizations::helpers::current_organization_id_for_user;

	let organization_id = current_organization_id_for_user(user_id).await?;

	let role = resolve_membership_role(user_id, organization_id).await?;
	let role = role.ok_or_else(|| {
		AppError::Authorization("User is not a member of the target organization".to_string())
	})?;

	if !allowed(role, action) {
		return Err(AppError::Authorization(format!(
			"Role '{}' is not permitted to perform this action",
			role.as_db_str()
		)));
	}

	Ok(organization_id)
}

/// Verify that `user_id` is permitted to perform `action` in the organization
/// identified by `org_slug`, returning that organization's id on success.
///
/// This is the canonical guard for org-scoped URL endpoints introduced by
/// issue #418 (`/api/orgs/{org_slug}/...`). It resolves the slug to an
/// `organization_id`, asserts membership, and checks the RBAC matrix.
///
/// # Errors
///
/// - 403 (`AppError::Authorization`) — slug unknown, user has no membership
///   in that org, or the user's role is not permitted to perform the action.
///   The slug-not-found and not-a-member cases both return 403 to prevent
///   org-existence enumeration.
/// - 500 (`AppError::Internal`) — database or data-integrity failure.
pub async fn require_permission_for_org(
	user_id: Uuid,
	org_slug: &str,
	action: Action,
) -> Result<i64, AppError> {
	use crate::apps::organizations::helpers::resolve_org_by_slug;

	let organization_id = resolve_org_by_slug(user_id, org_slug).await?;

	let role = resolve_membership_role(user_id, organization_id).await?;
	let role = role.ok_or_else(|| {
		AppError::Authorization("User is not a member of the target organization".to_string())
	})?;

	if !allowed(role, action) {
		return Err(AppError::Authorization(format!(
			"Role '{}' is not permitted to perform this action",
			role.as_db_str()
		)));
	}

	Ok(organization_id)
}
