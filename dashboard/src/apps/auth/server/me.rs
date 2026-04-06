//! Current-user server function for frontend session validation.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};

use crate::shared::UserInfo;

/// Return the currently authenticated user's information.
///
/// Authentication is handled by the cookie session middleware which
/// validates the `sessionid` cookie and sets `AuthState` in request
/// extensions. `AuthUser<User>` resolves the full user model from
/// the database via dependency injection.
#[server_fn]
pub async fn me(
	#[inject] reinhardt::AuthUser(user): reinhardt::AuthUser<crate::apps::auth::models::User>,
) -> Result<UserInfo, ServerFnError> {
	Ok(UserInfo::from(&user))
}
