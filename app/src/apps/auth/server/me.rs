//! Current-user server function for frontend session validation.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};

use crate::shared::UserInfo;

/// Return the currently authenticated user's information.
///
/// The WASM client sends the session token obtained from `login` or
/// `register`. On the server side the token is validated and the
/// corresponding user record is looked up. Returns an application
/// error if the token is missing, invalid, or the user no longer
/// exists.
#[server_fn]
pub async fn me(token: String) -> Result<UserInfo, ServerFnError> {
	use reinhardt::db::orm::{FilterOperator, FilterValue, Model};

	use crate::apps::auth::models::User;
	use crate::apps::auth::services;

	let (user_id, _username) = services::validate_raw_token(&token)
		.ok_or_else(|| ServerFnError::application("Session expired"))?;

	let user = User::objects()
		.filter(
			User::field_id(),
			FilterOperator::Eq,
			FilterValue::String(user_id),
		)
		.first()
		.await
		.map_err(|e| {
			tracing::error!("Failed to query user in me(): {e}");
			ServerFnError::application("Internal server error")
		})?
		.ok_or_else(|| ServerFnError::application("User not found"))?;

	Ok(services::user_to_info(&user))
}
