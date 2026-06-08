//! Personal Organization provisioning during user registration.
//!
//! Shared by auth server functions. This workflow provisions an
//! `Organization` + Owner `OrganizationMembership` for every
//! freshly-registered user so that organization-scoped resources
//! (Cluster, Deployment, ...) can resolve via
//! `current_organization_id_for_user`.
//!
//! Refs #415, #435.
//!
//! No `#[injectable_factory]` conversion (kent8192/reinhardt-cloud#599):
//! this module is a pure ORM-driven workflow. It does not read global
//! settings or environment variables; all inputs are function parameters
//! (`User` row, slug derivation rules). The framework-managed
//! `Organization::objects()` / `OrganizationMembership::objects()` ORM
//! entry points already encapsulate persistence, so wrapping the
//! function in a DI service would add a layer without removing any
//! global-state coupling.

use chrono::Utc;
use reinhardt::BaseUser;
use reinhardt::core::exception::Error as AppError;
use reinhardt::db::orm::Model;
use tracing::{error, info};

use crate::apps::auth::models::User;
use crate::apps::auth::services::email::EmailService;
use crate::apps::auth::services::token::{TokenPurpose, generate_token};
use crate::apps::organizations::models::{Organization, OrganizationMembership};
use crate::apps::organizations::roles::{
	MembershipRole, is_reserved_slug, sanitize_username_to_slug, validate_slug,
};
use crate::config::settings::ProjectSettings;

/// Register an inactive user, provision the personal organization, and send
/// the verification email.
pub async fn register_inactive_user(
	username: &str,
	email: &str,
	password: &str,
	email_service: &EmailService,
	settings: &ProjectSettings,
) -> Result<User, AppError> {
	let mut user = User::build()
		.username(username.trim().to_string())
		.email(email.trim().to_lowercase())
		.first_name(String::new())
		.last_name(String::new())
		.password_hash(None)
		.is_active(false)
		.is_staff(false)
		.is_superuser(false)
		.finish();
	user.set_password(password).map_err(|e| {
		error!("Password hashing failed during registration: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	let created = match User::objects().create(&user).await {
		Ok(user) => user,
		Err(e) => {
			let err_lower = e.to_string().to_lowercase();
			if err_lower.contains("unique") || err_lower.contains("duplicate") {
				let message = if err_lower.contains("auth_user_email_uniq") {
					"Email already exists"
				} else {
					"Username already exists"
				};
				return Err(AppError::Conflict(message.to_string()));
			}
			error!("Failed to create user in database: {e}");
			return Err(AppError::Internal("Internal server error".to_string()));
		}
	};

	provision_personal_organization(&created).await?;

	let token = generate_token(
		TokenPurpose::EmailVerification,
		&created.id,
		"",
		&settings.core.secret_key,
	);
	let port = std::env::var("PORT").unwrap_or_else(|_| "8000".to_string());
	let base_url = std::env::var("REINHARDT_CLOUD_BASE_URL")
		.unwrap_or_else(|_| format!("http://localhost:{port}"));
	let verification_url = format!("{base_url}/api/auth/verify-email/{token}/");

	if let Err(e) = email_service
		.send_verification_email(&created.email, &created.username, &verification_url)
		.await
	{
		error!(
			"Failed to send verification email to {}: {e}",
			created.email
		);
		if let Err(del_err) = User::objects().delete(created.id).await {
			error!("Failed to roll back user after email failure: {del_err}");
		}
		return Err(AppError::Internal(
			"Registration failed - please try again later".to_string(),
		));
	}
	info!("Verification email sent to {}", created.email);

	Ok(created)
}

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
	provision_personal_organization_inner(created, true).await
}

/// Create a Personal `Organization` and Owner membership for an existing
/// active user without rolling the user back on failure.
pub async fn ensure_personal_organization(user: &User) -> Result<(), AppError> {
	let existing = OrganizationMembership::objects()
		.filter(OrganizationMembership::field_user_id().eq(user.id.to_string()))
		.first()
		.await
		.map_err(|e| {
			error!("Failed to look up existing Personal Org membership: {e}");
			AppError::Internal("Internal server error".to_string())
		})?;
	if existing.is_some() {
		return Ok(());
	}

	provision_personal_organization_inner(user, false).await
}

async fn provision_personal_organization_inner(
	created: &User,
	rollback_on_failure: bool,
) -> Result<(), AppError> {
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
						if rollback_on_failure {
							rollback_user(created).await;
						}
						return Err(AppError::Internal("Internal server error".to_string()));
					}
				}
			} else {
				error!(
					"Failed to provision Personal Org for user {}: {e}",
					created.id
				);
				if rollback_on_failure {
					rollback_user(created).await;
				}
				return Err(AppError::Internal("Internal server error".to_string()));
			}
		}
	};

	let membership_input = OrganizationMembership::build()
		.organization(org.id.expect("created Organization has id"))
		.user(created.id)
		.role(MembershipRole::Owner.as_db_str().to_string())
		.finish();
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
		if rollback_on_failure {
			rollback_user(created).await;
		}
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
