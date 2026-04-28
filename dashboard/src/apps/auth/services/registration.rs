//! Personal Organization provisioning during user registration.
//!
//! Shared between the REST API (`apps::auth::views::register`) and the
//! frontend server function (`apps::auth::server::register`). Both flows
//! must provision an `Organization` + Owner `OrganizationMembership` for
//! every freshly-registered user so that organization-scoped resources
//! (Cluster, Deployment, …) can resolve via `current_organization_id_for_user`.
//!
//! Refs #415, #435.

use chrono::Utc;
use reinhardt::core::exception::Error as AppError;
use reinhardt::db::orm::Model;
use tracing::error;

use crate::apps::auth::models::User;
use crate::apps::organizations::models::{Organization, OrganizationMembership};
use crate::apps::organizations::roles::{
	MembershipRole, is_reserved_slug, sanitize_username_to_slug, validate_slug,
};

/// Create a Personal `Organization` and Owner `OrganizationMembership` for
/// a freshly-registered user. Rolls the user creation back on failure so
/// that the account never exists without an owning organization.
///
/// Slug derivation:
/// - DNS-1123 sanitize the username
/// - Fall back to `user-<short-uuid>` if the result is reserved or invalid
/// - On unique-violation (rare race between two simultaneous registrations),
///   retry once with a 6-char uuid suffix appended to the slug
pub async fn provision_personal_organization(created: &User) -> Result<(), AppError> {
	let now = Utc::now();
	let mut slug = sanitize_username_to_slug(&created.username);
	if is_reserved_slug(&slug) || validate_slug(&slug).is_err() {
		// Fall back to a user-<short-uuid> form so reserved/invalid slugs
		// do not block registration.
		let suffix = uuid::Uuid::new_v4().simple().to_string();
		slug = format!("user-{}", &suffix[..8]);
	}

	let org_input = Organization {
		id: None,
		slug: slug.clone(),
		name: created.username.clone(),
		created_by: created.id,
		created_at: now,
		updated_at: now,
	};

	// Try once with the derived slug. On unique-violation, retry once with a
	// uuid suffix.
	let org = match Organization::objects().create(&org_input).await {
		Ok(org) => org,
		Err(e) => {
			let err_lower = e.to_string().to_lowercase();
			if err_lower.contains("unique") || err_lower.contains("duplicate") {
				let suffix = uuid::Uuid::new_v4().simple().to_string();
				let retry = Organization {
					id: None,
					slug: format!("{}-{}", slug, &suffix[..6]),
					name: created.username.clone(),
					created_by: created.id,
					created_at: now,
					updated_at: now,
				};
				match Organization::objects().create(&retry).await {
					Ok(o) => o,
					Err(e2) => {
						error!(
							"Failed to provision Personal Org for user {} after retry: {e2}",
							created.id
						);
						rollback_user(created).await;
						return Err(AppError::Internal("Internal server error".to_string()));
					}
				}
			} else {
				error!(
					"Failed to provision Personal Org for user {}: {e}",
					created.id
				);
				rollback_user(created).await;
				return Err(AppError::Internal("Internal server error".to_string()));
			}
		}
	};

	let membership_input = OrganizationMembership {
		id: None,
		organization_id: org.id.expect("created Organization has id"),
		user_id: created.id,
		role: MembershipRole::Owner.as_db_str().to_string(),
		created_at: now,
	};
	if let Err(e) = OrganizationMembership::objects()
		.create(&membership_input)
		.await
	{
		error!(
			"Failed to provision Owner membership for user {} in org {}: {e}",
			created.id,
			org.id.unwrap_or_default()
		);
		// Best-effort rollback: delete the org we just created, then the user.
		if let Err(del_err) = Organization::objects()
			.delete(org.id.expect("created Organization has id"))
			.await
		{
			error!("Failed to roll back Organization after membership failure: {del_err}");
		}
		rollback_user(created).await;
		return Err(AppError::Internal("Internal server error".to_string()));
	}

	Ok(())
}

/// Best-effort delete of a user, used during Personal Org rollback.
async fn rollback_user(created: &User) {
	if let Err(del_err) = User::objects().delete(created.id).await {
		error!("Failed to roll back user after org provisioning failure: {del_err}");
	}
}
