//! The account page: the GitHub profile this Control Plane knows, whether the
//! User is Staff, and sign out.
//!
//! The data is the `Viewer` that the `current_viewer` server function derives
//! from the current `User` row (SR-07). Nothing else about the User is sent to
//! the browser: no tokens, no email address, no internal ID. An anonymous
//! browser is sent to the sign-in page (SR-09) by `signed_in_page`.
//!
//! `reinhardt-pages` checked: `ui::ActionButton` signs out (see
//! `sign_out_button`); `tables::Table` is for rows of records, and the profile
//! is a handful of facts, so they are a description list.

use reinhardt::pages::component::Page;
use reinhardt::pages::{page, t};

use crate::apps::accounts::client::components::shell::{Section, signed_in_page};
use crate::apps::accounts::client::components::sign_out_button::sign_out_button;
use crate::apps::accounts::client::style::STYLES;
use crate::apps::accounts::serializers::viewer::Viewer;
use crate::components::avatar::{AvatarSize, avatar, initials};
use crate::components::badge::chip;
use crate::components::button::{ButtonSize, ButtonVariant, button_class};
use crate::components::layout::signed_in::{page_head, page_section};

/// The value of the Staff row: the Staff badge for Staff, "No" for anyone else.
fn staff_value(viewer: &Viewer) -> Page {
	if viewer.is_staff {
		page!({
			span {
				class: STYLES.account_staff(),
				{ chip(t!("Staff")) }
				{ t!("You can operate this Control Plane across all Organizations.") }
			}
		})
	} else {
		page!({
			span { { t!("No") } }
		})
	}
}

/// The profile: the avatar, the name, and the facts about the account.
fn profile(viewer: &Viewer) -> Page {
	let picture = avatar(
		initials(&viewer.display_name, &viewer.github_login),
		AvatarSize::Large,
	);
	let name = viewer.display_name.clone();
	let login = t!("@{login}", login = viewer.github_login.clone());
	let staff = staff_value(viewer);
	page!({
		div {
			class: STYLES.account_card(),
			div {
				class: STYLES.account_head(),
				{ picture }
				p {
					class: STYLES.account_name(),
					{ name }
				}
			}
			dl {
				class: STYLES.account_facts(),
				dt { { t!("GitHub account") } }
				dd { { login } }
				dt { { t!("Staff") } }
				dd { { staff } }
			}
		}
	})
}

/// The content of the account page for `viewer`.
///
/// Call it inside a reactive scope: it creates the sign-out action.
pub fn account_content(viewer: &Viewer) -> Page {
	let head = page_head(
		t!("Account"),
		t!("The GitHub profile this Control Plane knows you by."),
	);
	let profile_section = page_section("account-profile", t!("Profile"), profile(viewer));
	let sign_out = sign_out_button(
		button_class(ButtonVariant::Secondary, ButtonSize::Regular),
		t!("Sign out"),
	);
	let session_body = page!({
		div {
			class: STYLES.account_session(),
			p { { t!("Signing out ends your session in this browser.") } }
			div { { sign_out } }
		}
	});
	let session_section = page_section("account-session", t!("Session"), session_body);
	Page::Fragment(vec![head, profile_section, session_section])
}

/// The account page, without its route registration (so tests can mount it).
pub fn account_page() -> Page {
	signed_in_page(Section::Account, account_content)
}

/// The account route.
#[reinhardt::pages::component("/account/", name = "account")]
pub fn account() -> Page {
	account_page()
}
