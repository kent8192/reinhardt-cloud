//! Current-user server function for frontend session validation.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};

#[cfg(native)]
use reinhardt::CurrentUser;

#[cfg(native)]
use crate::apps::auth::models::User;
use crate::shared::UserInfo;

/// Return the currently authenticated user's information.
///
/// Authentication is handled by the cookie session middleware which
/// validates the `sessionid` cookie and sets `AuthState` in request
/// extensions. `CurrentUser<User>` resolves the full user model from
/// the database via dependency injection.
#[server_fn]
pub async fn me(#[inject] CurrentUser(user): CurrentUser<User>) -> Result<UserInfo, ServerFnError> {
	Ok(UserInfo::from(&user))
}
