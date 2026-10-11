//! The page a Login Link opens (SR-16).
//!
//! The URL is `<origin>/sign-in/link/#<secret>`. Loading it fetches this
//! single-page application and nothing else: the browser does not send the
//! fragment, so no request, log line, or `Referer` carries the secret, and a
//! link previewer or scanner that only fetches the URL consumes nothing. The
//! secret is used only when the person presses the button, which is a POST from
//! this origin.
//!
//! `reinhardt-pages` checked: `ui::ActionButton` dispatches the confirmation and
//! `ui::ActionResultPanel` presents a rejection; `signed_out_layout` frames the
//! page. There is no form and no input: the secret is not typed.

use reinhardt::pages::component::{Component, IntoPage, Page};
use reinhardt::pages::reactive::{use_action, use_effect};
use reinhardt::pages::ui::{ActionButton, ActionResultPanel};
use reinhardt::pages::{NavigationType, deps, navigate, page, t};

use crate::apps::accounts::client::style::STYLES;
use crate::apps::accounts::serializers::login_link::LoginLinkOutcome;
use crate::apps::accounts::server_fn::consume_login_link::consume_login_link;
use crate::apps::accounts::urls::reverse;
use crate::components::alert::{AlertTone, alert};
use crate::components::browser::location_fragment;
use crate::components::button::{ButtonSize, ButtonVariant, button_class};
use crate::components::layout::signed_out::signed_out_layout;

/// The alert shown for a link that did not sign anyone in.
///
/// One message for every cause (unknown, used, expired, or a deactivated User):
/// the page must not tell them apart (SR-16).
pub fn rejected_alert() -> Page {
	alert(
		AlertTone::Danger,
		t!("This sign-in link did not work"),
		t!(
			"The link may already have been used, may have expired, or may be incomplete. Ask the operator who sent it to issue a new one."
		),
	)
}

/// The content column for a link whose secret is `token`.
///
/// `token` is `None` when the URL had no fragment, which no valid link has.
pub fn login_link_content(token: Option<String>) -> Page {
	let action = use_action(|token: String| async move { consume_login_link(token).await });
	use_effect(
		move || {
			if matches!(action.result(), Some(LoginLinkOutcome::SignedIn)) {
				// Replace, so the history entry that held the secret is gone.
				//
				// Ignoring the `Result` is safe: the User is already signed in
				// (the session cookie was set by the call that just succeeded), and
				// `navigate` fails only when no router is installed or the router
				// rejects the path, both of which leave the page where it is. Nothing
				// is lost by that; reloading the page, or opening `/`, shows the
				// signed-in landing. The same pattern ends the sign-out flow in
				// `home.rs`.
				let _ = navigate(reverse("home", &[]), NavigationType::Replace);
			}
			None::<fn()>
		},
		deps![action],
	);

	let body = match token {
		Some(token) => {
			let button = ActionButton::new(action, token, t!("Sign in"))
				.attr(
					"class",
					button_class(ButtonVariant::Primary, ButtonSize::Regular),
				)
				.render();
			let outcome = ActionResultPanel::new(action)
				.success(|outcome| match outcome {
					LoginLinkOutcome::SignedIn => Page::empty(),
					LoginLinkOutcome::Rejected => rejected_alert(),
				})
				// A failed call says nothing about the link, and nothing about why.
				.error(|_| rejected_alert())
				.render();
			page!({
				div { { button } }
				{ outcome }
			})
		}
		None => rejected_alert(),
	};

	page!({
		h1 {
			class: STYLES.title(),
			{ t!("Sign in with a Login Link") }
		}
		p {
			class: STYLES.lede(),
			{ t!(
				"A host operator of this Control Plane issued this link to sign you in. It works once and expires soon. Press the button to use it now."
			) }
		}
		{ body }
	})
}

/// The Login Link page, without its route registration (so tests can mount it).
pub fn login_link_page() -> Page {
	signed_out_layout(login_link_content(location_fragment()).into_page(), None)
}

/// The Login Link route.
#[reinhardt::pages::component("/sign-in/link/", name = "login-link")]
pub fn login_link() -> Page {
	login_link_page()
}
