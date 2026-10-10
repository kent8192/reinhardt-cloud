//! Tells the Dashboard who is signed in.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};

#[cfg(server)]
use crate::apps::accounts::models::User;
use crate::apps::accounts::serializers::viewer::Viewer;
#[cfg(server)]
use reinhardt::auth::CurrentUser;

/// The signed-in User, or `None` for an anonymous caller.
///
/// This is how the Dashboard decides between its pages and the sign-in page.
/// The answer is derived from the current `User` row on every call (SR-07), so
/// a deactivated User is anonymous at once.
#[server_fn]
pub async fn current_viewer(
	#[inject] user: Option<CurrentUser<User>>,
) -> Result<Option<Viewer>, ServerFnError> {
	Ok(user.map(|CurrentUser(user)| Viewer {
		github_login: user.github_login,
		display_name: user.display_name,
		avatar_url: user.avatar_url,
		is_staff: user.is_staff,
	}))
}
