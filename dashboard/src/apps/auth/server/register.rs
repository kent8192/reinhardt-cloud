//! Register server function for frontend user creation.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};

use crate::shared::{AuthResponse, UserInfo};

/// Create a new user account and set session cookie.
///
/// On the server side this creates a new user in the database with a
/// hashed password, creates a Redis session, and sets an HTTP-only
/// `sessionid` cookie. Returns an application error if the username
/// or email already exists.
#[server_fn]
pub async fn register(
	username: String,
	email: String,
	password: String,
	#[inject] http_request: reinhardt::pages::server_fn::ServerFnRequest,
) -> Result<AuthResponse, ServerFnError> {
	use reinhardt::BaseUser;
	use reinhardt::db::orm::Model;
	use reinhardt::http::ResponseCookies;
	use tracing::error;

	use crate::apps::auth::models::User;
	use crate::apps::auth::services;

	// Create user with hashed password
	let mut user = User::new(
		username.trim().to_string(),
		email.trim().to_string(),
		String::new(),
		String::new(),
		None,
		true,
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

	let session_id = services::create_session(&created).await.map_err(|err| {
		error!("Failed to create session during registration: {err}");
		ServerFnError::application("Internal server error")
	})?;

	// Set session cookie via ResponseCookies extension.
	let is_debug = crate::config::settings::get_settings().core.debug;
	let secure_flag = if is_debug { "" } else { "; Secure" };
	let cookie = format!(
		"sessionid={session_id}; HttpOnly; SameSite=Lax; Path=/{secure_flag}; Max-Age=86400"
	);
	let mut response_cookies = http_request
		.inner()
		.extensions
		.remove::<ResponseCookies>()
		.unwrap_or_default();
	response_cookies.add(cookie);
	http_request.inner().extensions.insert(response_cookies);

	let user_info = UserInfo::from(&created);
	Ok(AuthResponse {
		success: true,
		user: Some(user_info),
		token: None,
	})
}
