//! Register server function for frontend user creation.
//!
//! Creates a new user with `is_active = false` and sends a verification
//! email. The user must verify their email before they can log in.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};

use crate::shared::AuthResponse;

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
	csrf_token: String,
	#[inject] _http_request: reinhardt::pages::server_fn::ServerFnRequest,
	#[inject] email_service: reinhardt::di::Depends<
		crate::apps::auth::services::email::EmailService,
	>,
) -> Result<AuthResponse, ServerFnError> {
	#[cfg(native)]
	{
		use reinhardt::BaseUser;
		use reinhardt::db::orm::Model;
		use tracing::{error, info};

		use crate::apps::auth::models::User;
		use crate::apps::auth::services::registration::provision_personal_organization;
		use crate::apps::auth::services::token::{TokenPurpose, generate_token};
		use crate::shared::UserInfo;

		// CSRF validation runs in middleware before this handler, so the
		// token is consumed only to satisfy the form! contract here.
		let _ = csrf_token;

		let settings = crate::config::settings::get_settings();
		let secret_key = settings.core.secret_key.clone();

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

		// Provision a Personal Organization for the new user (refs #415, #435).
		// Shared with the REST register flow; slug derivation, retry, and
		// rollback are handled by the shared service.
		provision_personal_organization(&created)
			.await
			.map_err(|e| ServerFnError::application(e.to_string()))?;

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

		if let Err(e) = email_service
			.send_verification_email(&created.email, &created.username, &verification_url)
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
	#[cfg(wasm)]
	{
		// The #[server_fn] macro replaces this body with an HTTP POST stub on
		// wasm; this branch exists only so the function compiles as a single
		// declaration on both targets.
		let _ = (
			username,
			email,
			password,
			csrf_token,
			_http_request,
			email_service,
		);
		unreachable!("server_fn body is replaced on wasm")
	}
}
