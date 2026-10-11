//! Where a signed-in browser lands: the Overview.
//!
//! Nothing can be listed here yet: Organizations, Projects, and Clusters arrive
//! with later features, so the page says so instead of showing an empty list.
//! An anonymous browser is sent to the sign-in page (SR-09) by `signed_in_page`.
//!
//! `reinhardt-pages` checked: `ui::ResourcePanel` is for a resource that may be
//! empty; here the empty state is the page's whole content, drawn by the shared
//! `empty_state`.

use reinhardt::pages::component::Page;
use reinhardt::pages::t;

use crate::apps::accounts::client::components::shell::{Section, signed_in_page};
use crate::apps::accounts::serializers::viewer::Viewer;
use crate::components::empty_state::empty_state;
use crate::components::layout::signed_in::page_head;

/// The content of the Overview for `viewer`.
pub fn home_content(viewer: &Viewer) -> Page {
	let lede = t!("Welcome, {name}.", name = viewer.display_name.clone());
	let head = page_head(t!("Overview"), lede);
	let empty = empty_state(
		t!("Nothing to show yet"),
		t!("Your Organizations, Projects, and Clusters will appear here as they become available."),
		None,
	);
	Page::Fragment(vec![head, empty])
}

/// The Overview page, without its route registration (so tests can mount it).
pub fn home_page() -> Page {
	signed_in_page(Section::Overview, home_content)
}

/// The home route.
#[reinhardt::pages::component("/", name = "home")]
pub fn home() -> Page {
	home_page()
}
