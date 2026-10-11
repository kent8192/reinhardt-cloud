//! The frame of every signed-in page, and the guard of the private surface.
//!
//! A signed-in page renders its content through [`signed_in_page`]. The
//! single-page application and its assets are public, so the server cannot keep
//! an anonymous browser away from a private page (it still refuses every
//! private endpoint, SR-09); this guard is what stops a person from looking at
//! an empty private page: it asks who is signed in (derived from the current
//! `User` row on every call, SR-07), sends an anonymous browser to the sign-in
//! page, and renders nothing private before the answer arrives.
//!
//! `reinhardt-pages` checked: `ui::ResourcePanel` presents the viewer (loading,
//! anonymous, signed in, failed) and `ui::ActionButton` signs out (see
//! `sign_out_button`). The frame itself is `components::layout::signed_in`,
//! which explains why a page renders it instead of a `#[layout]` route.
//!
//! The primary navigation lists only what exists: when a feature adds a page,
//! it adds its entry here. The Organization switcher stays out until
//! Organizations exist.

use reinhardt::pages::component::Page;
use reinhardt::pages::t;
use reinhardt::pages::ui::ResourcePanel;

use crate::apps::accounts::client::components::user_menu::user_menu;
use crate::apps::accounts::client::hooks::viewer::{redirect_anonymous, use_viewer};
use crate::apps::accounts::serializers::viewer::Viewer;
use crate::apps::accounts::urls::reverse;
use crate::components::alert::{AlertTone, alert};
use crate::components::layout::signed_in::{NavItem, SignedInFrame, signed_in_layout};

/// The page a signed-in page belongs to, for the current-page marker of the
/// navigation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
	/// The landing page.
	Overview,
	/// The account page, reached from the User menu rather than the navigation.
	Account,
}

/// The frame around the page of `section` for `viewer`.
pub fn frame_for(viewer: &Viewer, section: Section) -> SignedInFrame {
	let home = reverse("home", &[]);
	SignedInFrame {
		home_href: home.clone(),
		// Organizations arrive later; until then nothing is rendered.
		switcher: None,
		nav: vec![NavItem {
			label: t!("Overview"),
			href: home,
			current: section == Section::Overview,
		}],
		user_menu: user_menu(viewer),
	}
}

/// Wraps the content that `content` builds for the signed-in User in the
/// application frame, once the browser is known to be signed in.
///
/// An anonymous browser is sent to the sign-in page and sees nothing of the
/// page.
///
/// `content` runs inside the success slot of the `ResourcePanel`, which is
/// re-run whenever the viewer resource changes state. Hooks it creates (such as
/// the sign-out action) are created again on each run and their state is lost;
/// that happens once, when the viewer first loads, because the resource does not
/// change state again after it succeeds.
pub fn signed_in_page(section: Section, content: impl Fn(&Viewer) -> Page + 'static) -> Page {
	let viewer = use_viewer();
	redirect_anonymous(&viewer);
	ResourcePanel::new(viewer)
		.loading(Page::empty)
		.empty_if(Option::is_none)
		.empty(|_| Page::empty())
		.success(move |viewer| {
			viewer.as_ref().map_or_else(Page::empty, |viewer| {
				signed_in_layout(frame_for(viewer, section), content(viewer))
			})
		})
		.error(|_| {
			alert(
				AlertTone::Danger,
				t!("The Dashboard could not load your account"),
				t!("Reload the page. If it keeps happening, ask Staff of this Control Plane."),
			)
		})
		.render()
}
