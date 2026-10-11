//! The sign-out button every signed-in page offers.
//!
//! Signing out calls the `sign_out` server function, which destroys the session
//! on the server (SR-08), and then leaves the private surface for the sign-in
//! page. `ui::ActionButton` dispatches the action and styles itself through the
//! class the caller passes. A failed sign-out is never silent: the session may
//! still be alive on the server, so `ui::ActionResultPanel` says so and the
//! User can try again.

use reinhardt::pages::component::{Component, Page};
use reinhardt::pages::reactive::{use_action, use_effect};
use reinhardt::pages::ui::{ActionButton, ActionResultPanel};
use reinhardt::pages::{NavigationType, TranslatedText, deps, navigate, page, t};

use crate::apps::accounts::server_fn::sign_out::sign_out;
use crate::apps::accounts::urls::reverse;
use crate::components::alert::{AlertTone, alert};

/// Renders a button that signs the User out and then shows the sign-in page.
///
/// `class` is the `class` attribute value of the button. Call it once per
/// button: each call owns its own action.
pub fn sign_out_button(class: String, label: TranslatedText) -> Page {
	let sign_out_action = use_action(|_: ()| async { sign_out().await });
	use_effect(
		move || {
			if sign_out_action.is_success() {
				// Ignoring the `Result` is safe: the session is already destroyed
				// on the server, and `navigate` fails only without an installed
				// router or on a rejected path, both of which leave the page where
				// it is. Reloading it then shows the sign-in page.
				let _ = navigate(reverse("sign-in", &[]), NavigationType::Replace);
			}
			None::<fn()>
		},
		deps![sign_out_action],
	);
	let button = ActionButton::new(sign_out_action, (), label)
		.attr("class", class)
		.render();
	let failure = ActionResultPanel::new(sign_out_action)
		.success(|()| Page::empty())
		.error(|_| {
			alert(
				AlertTone::Danger,
				t!("Sign-out did not complete"),
				t!("Your session may still be active. Try again."),
			)
		})
		.render();
	page!({
		{
			button
		}
		{ failure }
	})
}
