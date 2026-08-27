//! Personal Organization provisioning after email verification.
//!
//! Shared by auth server functions and verification handlers. This workflow
//! provisions an `Organization` + Owner `OrganizationMembership` only after
//! the user proves control of their email address so organization slugs cannot
//! be reserved by unverified registrations.
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
use reinhardt::db::orm::transaction::AtomicTransaction;
use reinhardt::db::orm::{Model, get_connection};
use tracing::{error, info};

use crate::apps::auth::models::User;
use crate::apps::auth::services::email::EmailService;
use crate::apps::auth::services::token::{TokenPurpose, generate_token};
use crate::apps::organizations::models::{Organization, OrganizationMembership};
use crate::apps::organizations::roles::{
	MembershipRole, is_reserved_slug, sanitize_username_to_slug, validate_slug,
};
use crate::config::ProjectSettings;

const MAX_ORG_SLUG_LEN: usize = 63;
type ProvisionResult<T> = Result<T, AppError>;

/// Register an inactive user and send the verification email.
///
/// Personal organization provisioning is intentionally deferred until email
/// verification succeeds so unauthenticated registrations cannot reserve
/// globally unique organization slugs.
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
				let message = if err_lower.contains("email_uniq")
					|| err_lower.contains("key (email)")
					|| err_lower.contains("(email)=")
				{
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
/// a verified user. Rolls the user creation back on failure when called from
/// legacy registration-recovery paths that request rollback semantics.
///
/// Slug derivation:
/// - DNS-1123 sanitize the username
/// - Fall back to `user-<short-uuid>` if the result is reserved or invalid
/// - On unique-violation (rare race between two simultaneous registrations),
///   retry once with a 6-char uuid suffix appended to the slug
pub async fn provision_personal_organization(created: &User) -> Result<(), AppError> {
	provision_personal_organization_inner(created, true)
		.await
		.map(|_| ())
}

/// Create a Personal `Organization` and Owner membership for an existing
/// active user without rolling the user back on failure.
pub async fn ensure_personal_organization(user: &User) -> Result<(), AppError> {
	provision_personal_organization_inner(user, false).await
}

async fn provision_personal_organization_inner(
	created: &User,
	rollback_on_failure: bool,
) -> Result<(), AppError> {
	let user_id = created.id;
	let username = created.username.clone();
	let conn = get_connection().await.map_err(|e| {
		error!("Failed to get database connection for Personal Org provisioning: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	let base_slug = personal_org_slug(&username);
	let mut result = Err(AppError::Internal(
		"Personal Org provisioning did not run".to_string(),
	));
	for attempt in 0..2 {
		let slug = if attempt == 0 {
			base_slug.clone()
		} else {
			retry_slug(&base_slug)
		};
		result = conn
			.atomic(async |tx| {
				provision_personal_organization_tx(tx, user_id, username.clone(), slug).await
			})
			.await;
		match &result {
			Ok(()) => break,
			Err(error) if attempt == 0 && is_unique_violation(error) => continue,
			Err(_) => break,
		}
	}

	if let Err(e) = result {
		error!(
			"Failed to provision Personal Org for user {}: {e}",
			created.id
		);
		if rollback_on_failure {
			rollback_user(created).await;
		}
		if matches!(&e, AppError::Authorization(_)) {
			return Err(e);
		}
		return Err(AppError::Internal("Internal server error".to_string()));
	}

	Ok(())
}

async fn provision_personal_organization_tx(
	tx: &mut AtomicTransaction,
	user_id: uuid::Uuid,
	username: String,
	slug: String,
) -> ProvisionResult<()> {
	let locked_users = User::objects()
		.filter(User::field_id().eq(user_id))
		.select_for_update()
		.all_with_executor(tx)
		.await?;
	if locked_users.len() != 1 {
		return Err(AppError::Internal(
			"Personal Org user no longer exists".to_string(),
		));
	}

	let existing = Organization::objects()
		.filter(Organization::field_created_by().eq(user_id))
		.order_by(&["created_at"])
		.all_with_db(tx)
		.await?
		.into_iter()
		.next();
	let (organization_id, existing_personal_org) = if let Some(organization) = existing {
		let organization_id = organization.id.ok_or_else(|| {
			AppError::Internal("Personal Organization is missing its primary key".to_string())
		})?;
		(organization_id, true)
	} else {
		let now = Utc::now();
		let organization = Organization {
			id: None,
			slug,
			name: username,
			created_by: user_id,
			created_at: now,
			updated_at: now,
		};
		let organization_id = Organization::objects()
			.create_with_conn(tx, &organization)
			.await?
			.id
			.ok_or_else(|| {
				AppError::Internal("created Personal Organization has no primary key".to_string())
			})?;
		(organization_id, false)
	};

	let existing_membership = OrganizationMembership::objects()
		.filter(OrganizationMembership::field_organization_id().eq(organization_id))
		.filter(OrganizationMembership::field_user_id().eq(user_id))
		.all_with_db(tx)
		.await?
		.into_iter()
		.next();
	if let Some(membership) = existing_membership {
		if membership.role != MembershipRole::Owner.as_db_str() {
			return Err(AppError::Internal(
				"Personal Organization membership is not owner".to_string(),
			));
		}
		return Ok(());
	}
	if existing_personal_org {
		return Err(AppError::Authorization(
			"Personal Organization membership is no longer active".to_string(),
		));
	}

	let membership = OrganizationMembership::build()
		.organization(organization_id)
		.user(user_id)
		.role(MembershipRole::Owner.as_db_str().to_string())
		.finish();
	OrganizationMembership::objects()
		.create_with_conn(tx, &membership)
		.await?;
	Ok(())
}

fn is_unique_violation(error: &AppError) -> bool {
	let message = error.to_string().to_lowercase();
	message.contains("unique") || message.contains("duplicate")
}

fn personal_org_slug(username: &str) -> String {
	let slug = sanitize_username_to_slug(username);
	if is_reserved_slug(&slug) || validate_slug(&slug).is_err() {
		let suffix = uuid::Uuid::new_v4().simple().to_string();
		return format!("user-{}", &suffix[..8]);
	}
	slug
}

fn retry_slug(slug: &str) -> String {
	let suffix = uuid::Uuid::new_v4().simple().to_string();
	let suffix = &suffix[..6];
	let prefix_len = MAX_ORG_SLUG_LEN - suffix.len() - 1;
	let prefix = if slug.len() > prefix_len {
		&slug[..prefix_len]
	} else {
		slug
	};
	format!("{prefix}-{suffix}")
}

/// Best-effort delete of a user, used during Personal Org rollback.
async fn rollback_user(created: &User) {
	if let Err(del_err) = User::objects().delete(created.id).await {
		error!("Failed to roll back user after org provisioning failure: {del_err}");
	}
}
