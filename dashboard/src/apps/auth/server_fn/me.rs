//! Current-user server function for frontend session validation.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};

use crate::shared::UserInfo;

/// Return the currently authenticated user's information.
///
/// Cookie restoration is followed by dashboard account validation, which
/// reloads the active user and sets `AuthState` in request extensions.
/// `CurrentUser<User>` then resolves the full user model through dependency
/// injection.
#[server_fn]
pub async fn me(
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
) -> Result<UserInfo, ServerFnError> {
	Ok(UserInfo::from(&user))
}
