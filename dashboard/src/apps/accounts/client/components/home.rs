//! Where a signed-in browser lands.
//!
//! The signed-in application shell replaces this page; until then it is the
//! guard of the private surface: an anonymous browser is sent to the sign-in
//! page (SR-09), a signed-in User sees who they are and can sign out.
//!
//! `reinhardt-pages` checked: `ui::ResourcePanel` presents the viewer
//! (loading, anonymous, signed in) and `ui::ActionButton` dispatches the
//! sign-out action with the design's button class.

use reinhardt::pages::component::{Component, Page};
use reinhardt::pages::reactive::{use_action, use_effect};
use reinhardt::pages::ui::{ActionButton, ResourcePanel};
use reinhardt::pages::{NavigationType, deps, navigate, page, t};

use crate::apps::accounts::client::hooks::viewer::{redirect_anonymous, use_viewer};
use crate::apps::accounts::serializers::viewer::Viewer;
use crate::apps::accounts::server_fn::sign_out::sign_out;
use crate::apps::accounts::urls::reverse;
use crate::components::button::{ButtonSize, ButtonVariant, button_class};

/// The signed-in landing content for `viewer`.
pub fn home_content(viewer: &Viewer, sign_out_button: Page) -> Page {
	let greeting = t!("Signed in as {name}", name = viewer.display_name.clone());
	let login = t!("@{login}", login = viewer.github_login.clone());
	page!({
		section {
			h1 { { greeting } }
			p { { login } }
			{ sign_out_button }
		}
	})
}

/// The signed-in landing page, without its route registration.
pub fn home_page() -> Page {
	let viewer = use_viewer();
	redirect_anonymous(&viewer);

	let sign_out_action = use_action(|_: ()| async { sign_out().await });
	use_effect(
		move || {
			if sign_out_action.is_success() {
				let _ = navigate(reverse("sign-in", &[]), NavigationType::Replace);
			}
			None::<fn()>
		},
		deps![sign_out_action],
	);
	let button = ActionButton::new(sign_out_action, (), t!("Sign out"))
		.attr(
			"class",
			button_class(ButtonVariant::Secondary, ButtonSize::Regular),
		)
		.render();

	ResourcePanel::new(viewer)
		.loading(Page::empty)
		.empty_if(Option::is_none)
		.empty(|_| Page::empty())
		.success(move |viewer| {
			viewer
				.as_ref()
				.map_or_else(Page::empty, |viewer| home_content(viewer, button.clone()))
		})
		.error(|_| Page::empty())
		.render()
}

/// The home route.
#[reinhardt::pages::component("/", name = "home")]
pub fn home() -> Page {
	home_page()
}
