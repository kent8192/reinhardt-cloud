//! Register server function for frontend user creation.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};

use crate::shared::AuthResponse;

/// Create a new user account and return session token.
///
/// On the server side this creates a new user in the database with a
/// hashed password, generates a JWT token, and returns both the token
/// and the new user information. Returns an application error if the
/// username or email already exists.
#[server_fn]
pub async fn register(
	username: String,
	email: String,
	password: String,
) -> Result<AuthResponse, ServerFnError> {
	use reinhardt::BaseUser;
	use reinhardt::db::orm::Model;
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

	// Attempt to create — database unique constraint prevents duplicates
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

	let token = services::create_session_token(&created).map_err(|err| {
		error!("Failed to create session token during registration: {err}");
		ServerFnError::application("Internal server error")
	})?;

	let user_info = services::user_to_info(&created);
	Ok(AuthResponse {
		success: true,
		user: Some(user_info),
		token: Some(token),
	})
}
