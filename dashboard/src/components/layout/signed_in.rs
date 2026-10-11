//! Frame of the pages a signed-in User sees.
//!
//! A sidebar holds the brand, an optional Organization switcher, the primary
//! navigation, and a foot with the User menu and the theme toggle; the page
//! content fills the `<main>` landmark beside it. On small screens the sidebar
//! becomes a header block above the content.
//!
//! `reinhardt-pages` checked: `router::Link` takes a plain `String` label and
//! has no active-link support, so the navigation links are anchors the client's
//! link interception routes. The menus are `<details>` disclosures. `ui` and
//! `tables` have no navigation or menu primitive; the theme toggle is the
//! shared `theme_toggle_as`.
//!
//! Workaround for kent8192/reinhardt-web#6727 (tracked in
//! kent8192/reinhardt-cloud#958): a `#[layout]` route keeps its shell mounted
//! while sibling routes change, and `reinhardt-pages` 0.4.0-alpha.20 gives a
//! mounted component no public way to read the current route (`use_router()`
//! exposes only push, replace, and navigate; `SpaRouter::current_path` is
//! reachable only through the `#[doc(hidden)]` `with_spa_router`), so a layout
//! cannot move the current-page marker of the navigation. Each page therefore
//! renders this frame itself through `signed_in_page`, which remounts the shell
//! on every navigation. Remove this workaround when the upstream issue is
//! resolved.
//!
//! Ideal implementation (without workaround):
//!   `#[layout("/", name = "signed-in-shell")]`
//!   `fn signed_in_shell(outlet: Outlet) -> Page`
//!   `    { signed_in_layout(frame_for(viewer), outlet) }`
//!   // The navigation entries are `Link`s that follow the reactive current
//!   // route and carry `aria-current="page"`; the pages are `#[layout]`
//!   // children that return only their content, and the shell stays mounted.
//!
//! The Organization switcher is `None` until Organizations exist; nothing is
//! rendered for it, not even an empty region.

use reinhardt::pages::component::{IntoPage, Page};
use reinhardt::pages::event::KeyDownEvent;
use reinhardt::pages::static_resolver::resolve_static;
use reinhardt::pages::{ClassList, TranslatedText, page, style_def, t};

use crate::components::browser;
use crate::components::button::ButtonVariant;
use crate::components::layout::{LOGO_MARK_PATH, LOGO_MARK_SIZE, MAIN_ID};
use crate::components::theme::theme_toggle_as;

/// Selector of the open menus the `Escape` key closes.
const OPEN_MENU_SELECTOR: &str = "details[data-menu][open]";

#[style_def]
pub static SIGNED_IN_STYLES: SignedInStyles = style! {
	globals {
		space_1: Length;
		space_2: Length;
		space_3: Length;
		space_4: Length;
		space_5: Length;
		space_6: Length;
		space_7: Length;
		space_8: Length;
		radius_md: Length;
		radius_lg: Length;
		border_width: Length;
		border_subtle: Color;
		border_default: Color;
		surface_raised: Color;
		surface_hover: Color;
		surface_inverse: Color;
		ink_inverse: Color;
		ink_strong: Color;
		ink_default: Color;
		ink_muted: Color;
		ink_subtle: Color;
		text_xs: Length;
		weight_medium: Number;
		weight_semibold: Number;
		weight_regular: Number;
		leading_tight: Number;
		leading_prose: Number;
		tracking_display: Length;
		tracking_heading: Length;
		text_lg: Length;
		text_2xl: Length;
		measure_prose: Length;
		sidebar_width: Length;
		sidebar_brand_mark_width: Length;
		measure_page: Length;
		control_height_lg: Length;
		skip_link_hidden_top: Length;
	}
	.app {
		display: grid;
		grid-template-columns: (globals.sidebar_width, 1fr);
		min-height: 100vh;
	}
	.skip {
		position: absolute;
		left: globals.space_3;
		top: globals.skip_link_hidden_top;
		z-index: 100;
		padding: (globals.space_2, globals.space_3);
		background: globals.surface_inverse;
		color: globals.ink_inverse;
		border-radius: globals.radius_md;
		&:focus {
			top: globals.space_3;
		}
	}
	.sidebar {
		position: sticky;
		top: 0;
		align-self: start;
		display: flex;
		flex-direction: column;
		gap: globals.space_5;
		height: 100vh;
		padding: (globals.space_4, globals.space_3);
		border-right: (globals.border_width, solid, globals.border_subtle);
	}
	.brand {
		display: flex;
		align-items: center;
		gap: globals.space_2;
		padding: (globals.space_1, globals.space_2);
		color: globals.ink_strong;
		font-weight: globals.weight_semibold;
		text-decoration: none;
		letter-spacing: globals.tracking_display;
		img {
			width: globals.sidebar_brand_mark_width;
			height: auto;
		}
	}
	.nav {
		display: flex;
		flex-direction: column;
		gap: globals.space_1;
	}
	.nav_link {
		display: flex;
		gap: globals.space_2;
		align-items: center;
		justify-content: space-between;
		padding: (globals.space_2, globals.space_3);
		border-radius: globals.radius_md;
		color: globals.ink_muted;
		text-decoration: none;
		font-weight: globals.weight_medium;
		&:hover {
			background: globals.surface_hover;
			color: globals.ink_strong;
		}
	}
	.nav_current {
		background: globals.surface_raised;
		color: globals.ink_strong;
	}
	.foot {
		margin-top: auto;
		display: flex;
		flex-direction: column;
		gap: globals.space_2;
	}
	.menu {
		position: relative;
	}
	.toggle {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: globals.space_2;
		width: 100%;
		min-height: globals.control_height_lg;
		padding: (globals.space_2, globals.space_3);
		background: globals.surface_raised;
		border: (globals.border_width, solid, globals.border_default);
		border-radius: globals.radius_md;
		color: globals.ink_strong;
		text-align: left;
	}
	.panel {
		display: flex;
		flex-direction: column;
		margin-top: globals.space_1;
		padding: globals.space_1;
		background: globals.surface_raised;
		border: (globals.border_width, solid, globals.border_default);
		border-radius: globals.radius_lg;
	}
	.item {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: globals.space_2;
		width: 100%;
		min-height: globals.control_height_lg;
		padding: (globals.space_2, globals.space_3);
		background: transparent;
		border: 0;
		border-radius: globals.radius_md;
		color: globals.ink_default;
		text-align: left;
		text-decoration: none;
		cursor: pointer;
		&:hover {
			background: globals.surface_hover;
		}
	}
	.name {
		display: block;
		font-weight: globals.weight_semibold;
		line-height: globals.leading_tight;
	}
	.sub {
		display: block;
		color: globals.ink_subtle;
		font-size: globals.text_xs;
	}
	.person {
		display: flex;
		align-items: center;
		gap: globals.space_3;
	}
	.theme {
		button {
			width: 100%;
			justify-content: flex-start;
			color: globals.ink_muted;
		}
	}
	.main {
		min-width: 0;
		padding: (globals.space_6, globals.space_7, globals.space_8);
	}
	.page {
		max-width: globals.measure_page;
		display: flex;
		flex-direction: column;
		gap: globals.space_6;
	}
	.page_head {
		display: flex;
		flex-direction: column;
		gap: globals.space_2;
	}
	.page_title {
		font-size: globals.text_2xl;
		font-weight: globals.weight_regular;
		letter-spacing: globals.tracking_display;
		line-height: globals.leading_tight;
		color: globals.ink_strong;
	}
	.page_lede {
		max-width: globals.measure_prose;
		color: globals.ink_muted;
		line-height: globals.leading_prose;
	}
	.section_title {
		margin-bottom: globals.space_3;
		font-size: globals.text_lg;
		font-weight: globals.weight_semibold;
		letter-spacing: globals.tracking_heading;
		color: globals.ink_strong;
	}
	@media (max-width: 62rem) {
		.app {
			grid-template-columns: unchecked_fn!(minmax(0, 1fr));
		}
		.sidebar {
			position: static;
			height: auto;
			gap: globals.space_3;
			border-right: 0;
			border-bottom: (globals.border_width, solid, globals.border_subtle);
		}
		.nav {
			flex-direction: row;
			flex-wrap: wrap;
		}
		.foot {
			flex-direction: row;
			align-items: center;
			margin-top: 0;
		}
		.main {
			padding: (globals.space_5, globals.space_4, globals.space_8);
		}
	}
};

/// One entry of the primary navigation.
pub struct NavItem {
	/// Visible label.
	pub label: TranslatedText,
	/// Destination; must come from a route reverse helper.
	pub href: String,
	/// Whether this is the page being shown.
	pub current: bool,
}

/// What surrounds the content of a signed-in page.
pub struct SignedInFrame {
	/// Where the brand links; must come from a route reverse helper.
	pub home_href: String,
	/// The Organization switcher. `None` renders nothing.
	pub switcher: Option<Page>,
	/// The primary navigation.
	pub nav: Vec<NavItem>,
	/// The User menu, built with [`menu`].
	pub user_menu: Page,
}

fn nav_link(item: NavItem) -> Page {
	let NavItem {
		label,
		href,
		current,
	} = item;
	if current {
		let classes =
			SIGNED_IN_STYLES.nav_link() + SIGNED_IN_STYLES.nav_current() + "rc-nav-current";
		page!({
			a {
				class: classes,
				href: href,
				aria_current: "page",
				{ label }
			}
		})
	} else {
		page!({
			a {
				class: SIGNED_IN_STYLES.nav_link(),
				href: href,
				{ label }
			}
		})
	}
}

/// Wraps `content` in the signed-in frame.
///
/// `content` becomes the contents of the `<main id="main">` landmark.
pub fn signed_in_layout(frame: SignedInFrame, content: impl IntoPage) -> Page {
	let SignedInFrame {
		home_href,
		switcher,
		nav,
		user_menu,
	} = frame;
	let content = content.into_page();
	let logo = resolve_static(LOGO_MARK_PATH);
	let (logo_width, logo_height) = (LOGO_MARK_SIZE.0.to_string(), LOGO_MARK_SIZE.1.to_string());
	let skip_target = format!("#{MAIN_ID}");
	let nav_label = t!("Primary").render_string();
	let sidebar_label = t!("Sidebar").render_string();
	let switcher = switcher.unwrap_or_else(Page::empty);
	let links: Vec<Page> = nav.into_iter().map(nav_link).collect();
	page!({
		div {
			class: SIGNED_IN_STYLES.app(),
			a {
				class: SIGNED_IN_STYLES.skip(),
				href: skip_target,
				{ t!("Skip to main content") }
			}
			aside {
				class: SIGNED_IN_STYLES.sidebar(),
				aria_label: sidebar_label,
				a {
					class: SIGNED_IN_STYLES.brand(),
					href: home_href,
					img {
						src: logo,
						alt: "",
						width: logo_width,
						height: logo_height,
					}
					{ t!("Reinhardt Cloud") }
				}
				{ switcher }
				nav {
					class: SIGNED_IN_STYLES.nav(),
					aria_label: nav_label,
					for link in links { { link } }
				}
				div {
					class: SIGNED_IN_STYLES.foot(),
					{ user_menu }
					div {
						class: SIGNED_IN_STYLES.theme(),
						{ theme_toggle_as(ButtonVariant::Quiet) }
					}
				}
			}
			main {
				id: MAIN_ID,
				class: SIGNED_IN_STYLES.main(),
				div {
					class: SIGNED_IN_STYLES.page(),
					{ content }
				}
			}
		}
	})
}

/// Returns the class list of a menu entry (a link or a button).
pub fn menu_item_classes() -> ClassList {
	SIGNED_IN_STYLES.item() + ""
}

/// Returns the `class` attribute value of a menu entry as an owned string, for
/// `ActionButton::attr("class", ..)`.
pub fn menu_item_class() -> String {
	std::borrow::Cow::from(menu_item_classes()).into_owned()
}

/// Renders a menu entry that navigates to `href`.
///
/// `href` must come from a route reverse helper.
pub fn menu_link(href: String, label: TranslatedText) -> Page {
	page!({
		a {
			class: menu_item_classes(),
			href: href,
			{ label }
		}
	})
}

/// Renders a disclosure menu: `summary` is the always-visible control and
/// `items` the entries shown when it is open.
///
/// `summary` is the content of the control (a name, a label); this wraps it
/// in the bordered toggle with its chevron. `Escape` closes the menu and
/// returns focus to the toggle.
pub fn menu(summary: Page, items: Vec<Page>) -> Page {
	let close_on_escape = move |event: KeyDownEvent| {
		if event.key() == "Escape" {
			browser::close_open_menus(OPEN_MENU_SELECTOR);
		}
	};
	page!({
		details {
			class: SIGNED_IN_STYLES.menu(),
			data_menu: "true",
			@keydown: close_on_escape,
			summary {
				class: SIGNED_IN_STYLES.toggle() + "rc-disclosure",
				{ summary }
			}
			div {
				class: SIGNED_IN_STYLES.panel() + "rc-elevation-1",
				for item in items { { item } }
			}
		}
	})
}

/// Renders a name over a smaller line of detail, as the Organization switcher
/// shows an Organization and its role.
pub fn name_and_detail(name: String, detail: String) -> Page {
	page!({
		span {
			span {
				class: SIGNED_IN_STYLES.name(),
				{ name }
			}
			span {
				class: SIGNED_IN_STYLES.sub(),
				{ detail }
			}
		}
	})
}

/// Renders a person: `avatar` beside their name and a line of detail.
///
/// `assistive_label` is read by assistive technology before the name and is
/// not shown, so a control made of this says what it is.
pub fn person(avatar: Page, assistive_label: TranslatedText, name: String, detail: String) -> Page {
	let who = name_and_detail(name, detail);
	page!({
		span {
			class: SIGNED_IN_STYLES.person(),
			{ avatar }
			span {
				span {
					class: "rc-visually-hidden",
					{ assistive_label }
				}
				{ who }
			}
		}
	})
}

/// Renders the heading of a page: its title and a line saying what it is for.
pub fn page_head(title: TranslatedText, lede: TranslatedText) -> Page {
	let title_classes = SIGNED_IN_STYLES.page_title() + "rc-font-display";
	page!({
		header {
			class: SIGNED_IN_STYLES.page_head(),
			h1 {
				class: title_classes,
				{ title }
			}
			p {
				class: SIGNED_IN_STYLES.page_lede(),
				{ lede }
			}
		}
	})
}

/// Renders a titled section of a page.
///
/// `id` must be unique in the document; the section is labelled by its title.
pub fn page_section(id: &str, title: TranslatedText, body: Page) -> Page {
	let title_id = format!("{id}-title");
	page!({
		section {
			aria_labelledby: title_id.clone(),
			h2 {
				id: title_id,
				class: SIGNED_IN_STYLES.section_title(),
				{ title }
			}
			{ body }
		}
	})
}
