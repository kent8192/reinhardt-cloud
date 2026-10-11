//! The User menu in the foot of the sidebar: who is signed in, the account
//! page, and sign out.
//!
//! No link to CLI Sessions appears: they belong to a later release.
//!
//! `reinhardt-pages` checked: `ui::ActionButton` signs out (see
//! `sign_out_button`); the menu is the shared `<details>` disclosure of
//! `components::layout::signed_in`, which sits in the sidebar's flow and covers
//! nothing, so it needs no `Portal`.

use reinhardt::pages::component::Page;
use reinhardt::pages::t;

use crate::apps::accounts::client::components::sign_out_button::sign_out_button;
use crate::apps::accounts::serializers::viewer::Viewer;
use crate::apps::accounts::urls::reverse;
use crate::components::avatar::{AvatarSize, avatar, initials};
use crate::components::layout::signed_in::{menu, menu_item_class, menu_link, person};

/// Renders the User menu for `viewer`.
pub fn user_menu(viewer: &Viewer) -> Page {
	let who = person(
		avatar(
			initials(&viewer.display_name, &viewer.github_login),
			AvatarSize::Regular,
		),
		t!("User menu"),
		viewer.display_name.clone(),
		t!("@{login}", login = viewer.github_login.clone()).render_string(),
	);
	menu(
		who,
		vec![
			menu_link(reverse("account", &[]), t!("Account")),
			sign_out_button(menu_item_class(), t!("Sign out")),
		],
	)
}
