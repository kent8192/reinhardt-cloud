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
use reinhardt::db::orm::connection::QueryValue;
use reinhardt::db::orm::transaction::TransactionScope;
use reinhardt::db::orm::{Model, get_connection};
use tracing::{error, info};

use crate::apps::auth::models::User;
use crate::apps::auth::services::email::EmailService;
use crate::apps::auth::services::token::{TokenPurpose, generate_token};
use crate::apps::organizations::roles::{
	MembershipRole, is_reserved_slug, sanitize_username_to_slug, validate_slug,
};
use crate::config::ProjectSettings;

const MAX_ORG_SLUG_LEN: usize = 63;
type ProvisionResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

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

	let mut tx = TransactionScope::begin(&conn).await.map_err(|e| {
		error!("Failed to begin Personal Org provisioning transaction: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	let result = provision_personal_organization_tx(&mut tx, user_id, username).await;

	if let Err(e) = result {
		error!(
			"Failed to provision Personal Org for user {}: {e}",
			created.id
		);
		if let Err(rollback_err) = tx.rollback().await {
			error!("Failed to roll back Personal Org provisioning transaction: {rollback_err}");
		}
		if rollback_on_failure {
			rollback_user(created).await;
		}
		return Err(AppError::Internal("Internal server error".to_string()));
	}

	tx.commit().await.map_err(|e| {
		error!("Failed to commit Personal Org provisioning transaction: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	Ok(())
}

async fn provision_personal_organization_tx(
	tx: &mut TransactionScope,
	user_id: uuid::Uuid,
	username: String,
) -> ProvisionResult<()> {
	lock_personal_org_provisioning(tx, user_id).await?;
	if find_personal_organization_id(tx, user_id).await?.is_some() {
		return Ok(());
	}

	let now = Utc::now();
	let slug = personal_org_slug(&username);
	let org_id = if let Some(org_id) =
		insert_personal_organization(tx, user_id, &username, &slug, now).await?
	{
		org_id
	} else {
		if find_personal_organization_id(tx, user_id).await?.is_some() {
			return Ok(());
		}
		let retry = retry_slug(&slug);
		insert_personal_organization(tx, user_id, &username, &retry, now)
			.await?
			.ok_or_else(|| {
				std::io::Error::other("retry Personal Organization slug also conflicted")
			})?
	};

	if !insert_owner_membership(tx, user_id, org_id, now).await?
		&& find_personal_organization_id(tx, user_id).await?.is_none()
	{
		return Err(std::io::Error::other(
			"Owner membership already existed for a non-Personal Organization",
		)
		.into());
	}

	Ok(())
}

async fn lock_personal_org_provisioning(
	tx: &mut TransactionScope,
	user_id: uuid::Uuid,
) -> ProvisionResult<()> {
	tx.execute(
		"SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
		vec![QueryValue::String(user_id.to_string())],
	)
	.await?;
	Ok(())
}

async fn find_personal_organization_id(
	tx: &mut TransactionScope,
	user_id: uuid::Uuid,
) -> ProvisionResult<Option<i64>> {
	let row = tx
		.query_optional(
			"SELECT o.id \
			 FROM organizations o \
			 JOIN organization_memberships m ON m.organization_id = o.id \
			 WHERE o.created_by = $1 \
			   AND m.user_id = $1 \
			   AND m.role = $2 \
			 ORDER BY o.created_at ASC \
			 LIMIT 1",
			vec![
				QueryValue::Uuid(user_id),
				QueryValue::String(MembershipRole::Owner.as_db_str().to_string()),
			],
		)
		.await?;
	Ok(row.and_then(|row| row.get("id")))
}

async fn insert_personal_organization(
	tx: &mut TransactionScope,
	user_id: uuid::Uuid,
	username: &str,
	slug: &str,
	now: chrono::DateTime<Utc>,
) -> ProvisionResult<Option<i64>> {
	let row = tx
		.query_optional(
			"INSERT INTO organizations (slug, name, created_by, created_at, updated_at) \
			 VALUES ($1, $2, $3, $4, $5) \
			 ON CONFLICT (slug) DO NOTHING \
			 RETURNING id",
			vec![
				QueryValue::String(slug.to_string()),
				QueryValue::String(username.to_string()),
				QueryValue::Uuid(user_id),
				QueryValue::Timestamp(now),
				QueryValue::Timestamp(now),
			],
		)
		.await?;
	row.map(|row| {
		row.get("id")
			.ok_or_else(|| std::io::Error::other("created Personal Organization did not return id"))
	})
	.transpose()
	.map_err(Into::into)
}

async fn insert_owner_membership(
	tx: &mut TransactionScope,
	user_id: uuid::Uuid,
	org_id: i64,
	now: chrono::DateTime<Utc>,
) -> ProvisionResult<bool> {
	let rows = tx
		.execute(
			"INSERT INTO organization_memberships (organization_id, user_id, role, created_at) \
		 VALUES ($1, $2, $3, $4) \
		 ON CONFLICT ON CONSTRAINT organization_memberships_org_user_unique DO NOTHING",
			vec![
				QueryValue::Int(org_id),
				QueryValue::Uuid(user_id),
				QueryValue::String(MembershipRole::Owner.as_db_str().to_string()),
				QueryValue::Timestamp(now),
			],
		)
		.await?;
	Ok(rows > 0)
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
