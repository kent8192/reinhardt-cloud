//! The sign-in page (Variant A of the approved design).
//!
//! `reinhardt-pages` checked: `ui::ResourcePanel` presents the notice the last
//! failed attempt left (loading, nothing to show, notice, error) and the shared
//! `alert` styles its content; `signed_out_layout` frames the page. There is
//! no form: GitHub is the only way in (SR-01), so the single action is a
//! navigation to the server route that starts the flow, drawn as a link
//! because `ui::ActionButton` dispatches an `Action` and renders no anchor.

use reinhardt::pages::component::{IntoPage, Page};
use reinhardt::pages::reactive::use_resource;
use reinhardt::pages::ui::ResourcePanel;
use reinhardt::pages::{deps, page, t};

use crate::apps::accounts::client::components::sign_in_preview::sign_in_preview;
use crate::apps::accounts::client::style::STYLES;
use crate::apps::accounts::serializers::sign_in::SignInNotice;
use crate::apps::accounts::server_fn::take_sign_in_notice::take_sign_in_notice;
use crate::apps::accounts::urls::paths::GITHUB_SIGN_IN_PATH;
use crate::components::alert::{AlertTone, alert};
use crate::components::button::{ButtonSize, ButtonVariant, external_link_button};
use crate::components::layout::signed_out::signed_out_layout;

/// The alert for the outcome of the last attempt.
///
/// A rejected account is told only that it has no Invitation, whichever sign-up
/// policy refused it, so the policy's contents stay private (SR-19). Any other
/// failure is not explained.
pub fn notice_alert(notice: &SignInNotice) -> Page {
	match notice {
		SignInNotice::NotInvited { login } => alert(
			AlertTone::Warning,
			t!("No Invitation found for @{login}", login = login.clone()),
			t!(
				"This Control Plane accepts new Users by Invitation only. Ask an owner of your Organization to send an Invitation to your GitHub account, then sign in again."
			),
		),
		SignInNotice::Failed => alert(
			AlertTone::Danger,
			t!("Sign-in did not complete"),
			t!(
				"GitHub did not complete the sign-in. Try again, and ask Staff of this Control Plane if it keeps happening."
			),
		),
	}
}

/// The content column: title, lede, the GitHub action, the notice, and the
/// footnote.
pub fn sign_in_content(notice: Page) -> Page {
	let action = external_link_button(
		GITHUB_SIGN_IN_PATH.to_owned(),
		t!("Continue with GitHub"),
		ButtonVariant::Github,
		ButtonSize::Regular,
	);
	page!({
		h1 {
			class: STYLES.title(),
			{ t!("Run your Reinhardt Projects on your own Clusters.") }
		}
		p {
			class: STYLES.lede(),
			{ t!(
				"Register a Kubernetes Cluster, connect a GitHub repository, and every push becomes a Deployment you can watch."
			) }
		}
		div { { action } }
		{ notice }
		p {
			class: STYLES.foot(),
			{ t!("GitHub is the only way to sign in. There are no passwords to set or reset.") }
		}
	})
}

/// The sign-in page, without its route registration (so tests can mount it).
pub fn sign_in_page() -> Page {
	let notice = use_resource(|| async { take_sign_in_notice().await }, deps![]);
	let area = ResourcePanel::new(notice)
		.loading(Page::empty)
		.empty_if(Option::is_none)
		.empty(|_| Page::empty())
		.success(|notice| notice.as_ref().map_or_else(Page::empty, notice_alert))
		.error(|_| Page::empty())
		.render();
	signed_out_layout(sign_in_content(area).into_page(), Some(sign_in_preview()))
}

/// The sign-in route.
#[reinhardt::pages::component("/sign-in/", name = "sign-in")]
pub fn sign_in() -> Page {
	sign_in_page()
}
