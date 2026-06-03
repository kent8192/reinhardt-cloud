//! Organization helper functions for view-layer use.
//!
//! `current_organization_id_for_user` is kept for the deprecated flat-URL
//! redirect middleware that issues 307s during the one-release transition
//! window introduced by issue #418.
//!
//! `resolve_org_by_slug` is the canonical helper for org-scoped endpoints
//! introduced by issue #418 (`/api/orgs/{org_slug}/...`). It validates that
//! the requesting user is a member of the specified organization, returning
//! 403 rather than 404 on unknown slugs to prevent org-existence leakage.

use reinhardt::Model;
use reinhardt::core::exception::Error as AppError;
use tracing::error;
use uuid::Uuid;

use crate::apps::organizations::models::{Organization, OrganizationMembership};

/// Returns the `organization_id` of the first membership the user holds,
/// ordered by `created_at` ascending. This is the user's Personal Org for
/// users registered after #415.
///
/// Returns `AppError::NotFound` if the user has no memberships. This should
/// be impossible for users registered after #415 (the registration view
/// always provisions a Personal Org); a stranded account indicates either
/// a partial-failure rollback bug or a pre-#415 dev account that needs
/// re-registration.
///
/// Used only by the deprecated flat-URL redirect middleware. New code should
/// call `resolve_org_by_slug` instead.
pub async fn current_organization_id_for_user(user_id: Uuid) -> Result<i64, AppError> {
	let m = OrganizationMembership::objects()
		.filter(OrganizationMembership::field_user_id().eq(user_id.to_string()))
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

/// Resolve an org slug to its `organization_id`, asserting membership.
///
/// Returns the `organization_id` when `user_id` is a member of the org
/// identified by `slug`. Returns `AppError::Authorization` (403) for both
/// unknown slugs and slugs where the user has no membership — intentionally
/// indistinguishable to prevent org-existence enumeration.
///
/// # Errors
///
/// - 403 (`AppError::Authorization`) — slug unknown, or user has no
///   membership in that org.
/// - 500 (`AppError::Internal`) — database failure.
pub async fn resolve_org_by_slug(user_id: Uuid, slug: &str) -> Result<i64, AppError> {
	let org = Organization::objects()
		.filter(Organization::field_slug().eq(slug.to_string()))
		.first()
		.await
		.map_err(|e| {
			error!("Failed to look up organization by slug: {e}");
			AppError::Internal("Internal server error".to_string())
		})?;

	let Some(org) = org else {
		// Return 403, not 404, to avoid leaking org existence.
		return Err(AppError::Authorization(
			"Organization not found or access denied".to_string(),
		));
	};

	let org_id = org.id.ok_or_else(|| {
		error!("Organization row missing primary key");
		AppError::Internal("Internal server error".to_string())
	})?;

	let membership = OrganizationMembership::objects()
		.filter(OrganizationMembership::field_user_id().eq(user_id.to_string()))
		.filter(OrganizationMembership::field_organization_id().eq(org_id))
		.first()
		.await
		.map_err(|e| {
			error!("Failed to look up organization membership: {e}");
			AppError::Internal("Internal server error".to_string())
		})?;

	if membership.is_none() {
		// Same 403 for no-membership — indistinguishable from unknown slug.
		return Err(AppError::Authorization(
			"Organization not found or access denied".to_string(),
		));
	}

	Ok(org_id)
}
