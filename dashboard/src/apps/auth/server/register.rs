//! Register server function for frontend user creation.
//!
//! Creates a new user with `is_active = false` and sends a verification
//! email. The user must verify their email before they can log in.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};

use crate::shared::{AuthResponse, UserInfo};

/// Create a new user account with email verification.
///
/// On the server side this creates a new user in the database with a
/// hashed password and `is_active = false`, then sends a verification
/// email. No session cookie is set — the user must verify their email
/// first. Returns an application error if the username or email already exists.
#[server_fn]
pub async fn register(
	username: String,
	email: String,
	password: String,
	#[inject] _http_request: reinhardt::pages::server_fn::ServerFnRequest,
) -> Result<AuthResponse, ServerFnError> {
	use reinhardt::BaseUser;
	use reinhardt::db::orm::Model;
	use tracing::{error, info};

	use crate::apps::auth::models::User;
	use crate::apps::auth::services::email::{get_email_backend, send_verification_email};
	use crate::apps::auth::services::token::{TokenPurpose, generate_token};

	let settings = crate::config::settings::get_settings();
	let secret_key = settings.core.secret_key.clone();
	let from_email = settings.email.from_email.clone();

	// Create user as inactive — requires email verification to activate
	let mut user = User::new(
		username.trim().to_string(),
		email.trim().to_lowercase(),
		String::new(),
		String::new(),
		None,
		false,
		false,
		false,
	);
	user.set_password(&password).map_err(|e| {
		error!("Password hashing failed during registration: {e}");
		ServerFnError::application("Internal server error")
	})?;

	// Attempt to create -- database unique constraint prevents duplicates
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
				return Err(ServerFnError::application(message));
			}
			error!("Failed to create user in database: {e}");
			return Err(ServerFnError::application("Internal server error"));
		}
	};

	// Provision a Personal Organization for the new user.
	//
	// Slug is derived from the username via DNS-1123 sanitization. On
	// reserved-name collision (e.g., username "admin"), we fall back to a
	// `user-<short-uuid>` slug. On unique-violation we retry with a fresh
	// uuid suffix once.
	provision_personal_organization(&created).await?;

	// Generate verification token and send email
	let token = generate_token(
		TokenPurpose::EmailVerification,
		&created.id,
		"",
		&secret_key,
	);

	let port = std::env::var("PORT").unwrap_or_else(|_| "8000".to_string());
	let base_url = std::env::var("REINHARDT_CLOUD_BASE_URL")
		.unwrap_or_else(|_| format!("http://localhost:{port}"));
	let verification_url = format!("{base_url}/api/auth/verify-email/{token}/");

	let backend = get_email_backend().map_err(|e| {
		error!("Failed to create email backend: {e}");
		ServerFnError::application("Internal server error")
	})?;

	if let Err(e) = send_verification_email(
		&created.email,
		&created.username,
		&verification_url,
		backend.as_ref(),
		&from_email,
	)
	.await
	{
		error!(
			"Failed to send verification email to {}: {e}",
			created.email
		);
		// Roll back user creation to avoid stranding an inactive account
		if let Err(del_err) = User::objects().delete(created.id).await {
			error!("Failed to roll back user after email failure: {del_err}");
		}
		return Err(ServerFnError::application(
			"Registration failed — please try again later",
		));
	}
	info!("Verification email sent to {}", created.email);

	// No session cookie — user must verify email first
	let user_info = UserInfo::from(&created);
	Ok(AuthResponse {
		success: true,
		user: Some(user_info),
	})
}

/// Create a Personal `Organization` and an `Owner` `OrganizationMembership`
/// for a freshly-registered user. Rolls the user creation back on failure so
/// that the account never exists without an owning organization.
///
/// Refs #415
async fn provision_personal_organization(
	created: &crate::apps::auth::models::User,
) -> Result<(), reinhardt::pages::server_fn::ServerFnError> {
	use chrono::Utc;
	use reinhardt::db::orm::Model;
	use reinhardt::pages::server_fn::ServerFnError;
	use tracing::error;

	use crate::apps::organizations::models::{Organization, OrganizationMembership};
	use crate::apps::organizations::roles::{
		MembershipRole, is_reserved_slug, sanitize_username_to_slug, validate_slug,
	};

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
		created_at: now,
		updated_at: now,
	};

	// Try once with the derived slug. On unique-violation (rare collision
	// between two simultaneous registrations), retry once with a uuid suffix.
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
						return Err(ServerFnError::application("Internal server error"));
					}
				}
			} else {
				error!(
					"Failed to provision Personal Org for user {}: {e}",
					created.id
				);
				rollback_user(created).await;
				return Err(ServerFnError::application("Internal server error"));
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
		return Err(ServerFnError::application("Internal server error"));
	}

	Ok(())
}

/// Best-effort delete of a user, used during Personal Org rollback.
async fn rollback_user(created: &crate::apps::auth::models::User) {
	use reinhardt::db::orm::Model;
	use tracing::error;

	use crate::apps::auth::models::User;

	if let Err(del_err) = User::objects().delete(created.id).await {
		error!("Failed to roll back user after org provisioning failure: {del_err}");
	}
}
