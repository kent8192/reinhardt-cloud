//! Who is signed in, as the Dashboard sees it.
//!
//! The single-page application shell and its assets are public, so a private
//! page cannot rely on the server to keep an anonymous browser away from it.
//! [`use_viewer`] asks the server who the caller is (the answer is derived from
//! the current `User` row on every call, SR-07) and [`redirect_anonymous`]
//! sends an anonymous browser to the sign-in page instead of rendering the
//! page (SR-09). The server enforces the same rule on every endpoint; this is
//! what keeps a person from looking at an empty private page.

use reinhardt::pages::reactive::{Resource, ResourceState, use_effect, use_resource};
use reinhardt::pages::server_fn::ServerFnError;
use reinhardt::pages::{NavigationType, deps, navigate};

use crate::apps::accounts::serializers::viewer::Viewer;
use crate::apps::accounts::server_fn::current_viewer::current_viewer;
use crate::apps::accounts::urls::reverse;

/// Load the signed-in User, or `None` for an anonymous caller.
pub fn use_viewer() -> Resource<Option<Viewer>, ServerFnError> {
	use_resource(|| async { current_viewer().await }, deps![])
}

/// Navigate to the sign-in page once `viewer` reports an anonymous caller.
pub fn redirect_anonymous(viewer: &Resource<Option<Viewer>, ServerFnError>) {
	let viewer = *viewer;
	use_effect(
		move || {
			if matches!(viewer.get(), ResourceState::Success(None)) {
				let _ = navigate(reverse("sign-in", &[]), NavigationType::Replace);
			}
			None::<fn()>
		},
		deps![viewer],
	);
}
