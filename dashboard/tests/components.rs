//! Native tests for the design applied to `reinhardt-pages` primitives and for
//! the custom components that have no primitive.

use std::borrow::Cow;
use std::cell::Cell;
use std::rc::Rc;

use cloud_control_plane::components::alert::{ALERT_STYLES, AlertTone, alert, alert_classes};
use cloud_control_plane::components::avatar::{
	AVATAR_STYLES, AvatarSize, avatar, avatar_classes, initials,
};
use cloud_control_plane::components::badge::{BadgeStatus, badge, badge_classes, chip};
use cloud_control_plane::components::button::{
	ButtonProps, ButtonSize, ButtonVariant, button, button_class, button_classes,
	external_link_button, link_button,
};
use cloud_control_plane::components::code_block::{CODE_BLOCK_STYLES, code_block};
use cloud_control_plane::components::dialog::{
	DIALOG_STYLES, DialogProps, dialog_classes, dialog_view, open_dialog,
};
use cloud_control_plane::components::empty_state::empty_state;
use cloud_control_plane::components::form_styles::{FORM_STYLES, form_classes};
use cloud_control_plane::components::layout::signed_in::{
	NavItem, SignedInFrame, menu, menu_item_class, menu_link, page_head, page_section, person,
	signed_in_layout,
};
use cloud_control_plane::components::layout::signed_out::{MAIN_ID, signed_out_layout};
use cloud_control_plane::components::theme::{Theme, theme_toggle};
use cloud_control_plane::i18n::i18n_context;
use reinhardt::pages::component::{Component, Outlet, Page};
use reinhardt::pages::event::ClickEvent;
use reinhardt::pages::i18n::provide_i18n_context;
use reinhardt::pages::reactive::{ReactiveScope, ResourceState, use_action, use_resource};
use reinhardt::pages::testing::component::{Role, render};
use reinhardt::pages::ui::{ActionButton, ActionResultPanel, ResourcePanel};
use reinhardt::pages::{Callback, deps, page, t};
use rstest::rstest;
use serial_test::serial;

/// Runs `test` inside a reactive scope with the English i18n context installed.
fn within_ui(test: impl FnOnce()) {
	ReactiveScope::run(|| {
		let _i18n = provide_i18n_context(i18n_context());
		test();
	});
}

/// Returns the serialized `class` attribute value of a class token or list.
fn class_value(classes: impl Into<Cow<'static, str>>) -> String {
	classes.into().into_owned()
}

#[rstest]
#[case(ButtonVariant::Secondary, ButtonSize::Regular, 1)]
#[case(ButtonVariant::Primary, ButtonSize::Regular, 2)]
#[case(ButtonVariant::QuietDanger, ButtonSize::Small, 4)]
fn button_classes_compose_the_base_variant_and_size(
	#[case] variant: ButtonVariant,
	#[case] size: ButtonSize,
	#[case] expected_count: usize,
) {
	// Arrange
	let classes = button_classes(variant, size);

	// Act
	let value = class_value(classes);

	// Assert
	assert_eq!(value.split(' ').count(), expected_count);
	assert!(value.starts_with("btn--rs-"));
}

#[rstest]
#[serial(i18n)]
fn button_renders_its_translated_label_and_dispatches_clicks() {
	within_ui(|| {
		// Arrange
		let clicks = Rc::new(Cell::new(0_u32));
		let handler = Callback::new({
			let clicks = Rc::clone(&clicks);
			move |_event: ClickEvent| clicks.set(clicks.get() + 1)
		});
		let props = ButtonProps::new(t!("Copy"))
			.variant(ButtonVariant::Primary)
			.on_click(handler);
		let screen = render(button(props));

		// Act
		screen.get_by_role(Role::Button, "Copy").click();

		// Assert
		assert_eq!(clicks.get(), 1);
		assert_eq!(
			screen.pretty(),
			format!(
				"<button class=\"{}\" type=\"button\">\n  Copy\n</button>\n",
				class_value(button_classes(ButtonVariant::Primary, ButtonSize::Regular)),
			)
		);
	});
}

#[rstest]
#[serial(i18n)]
fn button_defaults_to_type_button_and_can_submit() {
	within_ui(|| {
		// Arrange
		let plain = render(button(ButtonProps::new(t!("Copy"))));
		let submit = render(button(ButtonProps::new(t!("Copy")).submit()));

		// Act
		let plain_html = plain.pretty();
		let submit_html = submit.pretty();

		// Assert
		assert!(plain_html.contains("type=\"button\""));
		assert!(submit_html.contains("type=\"submit\""));
	});
}

#[rstest]
#[serial(i18n)]
fn link_button_points_at_the_given_href() {
	within_ui(|| {
		// Arrange
		let href = "/sign-in/".to_owned();

		// Act
		let screen = render(link_button(
			href,
			t!("Copy"),
			ButtonVariant::Github,
			ButtonSize::Regular,
		));

		// Assert
		assert_eq!(
			screen.pretty(),
			format!(
				"<a class=\"{}\" href=\"/sign-in/\">\n  Copy\n</a>\n",
				class_value(button_classes(ButtonVariant::Github, ButtonSize::Regular)),
			)
		);
	});
}

#[rstest]
#[serial(i18n)]
fn external_link_button_opts_out_of_client_link_interception() {
	within_ui(|| {
		// Arrange
		let href = "/api/auth/github/".to_owned();

		// Act
		let screen = render(external_link_button(
			href,
			t!("Copy"),
			ButtonVariant::Github,
			ButtonSize::Regular,
		));

		// Assert
		assert_eq!(
			screen.pretty(),
			format!(
				"<a class=\"{}\" href=\"/api/auth/github/\" rel=\"external\">\n  Copy\n</a>\n",
				class_value(button_classes(ButtonVariant::Github, ButtonSize::Regular)),
			)
		);
	});
}

#[rstest]
#[case(BadgeStatus::Success, false)]
#[case(BadgeStatus::Progress, true)]
#[case(BadgeStatus::Warning, false)]
#[case(BadgeStatus::Danger, false)]
#[case(BadgeStatus::Neutral, false)]
fn only_the_progress_badge_spins(#[case] status: BadgeStatus, #[case] spins: bool) {
	// Arrange
	let value = class_value(badge_classes(status));

	// Act
	let classes: Vec<&str> = value.split(' ').collect();

	// Assert
	assert_eq!(classes.contains(&"rc-spin"), spins);
	assert!(classes[0].starts_with("badge--rs-"));
	assert_eq!(classes.len(), if spins { 3 } else { 2 });
}

#[rstest]
#[serial(i18n)]
fn badge_and_chip_render_their_labels() {
	within_ui(|| {
		// Arrange
		let badge_screen = render(badge(BadgeStatus::Success, t!("Copied")));
		let chip_screen = render(chip(t!("Copy")));

		// Act
		let badge_text = badge_screen.get_by_text("Copied").text();
		let chip_text = chip_screen.get_by_text("Copy").text();

		// Assert
		assert_eq!(badge_text, "Copied");
		assert_eq!(chip_text, "Copy");
	});
}

#[rstest]
#[case(AlertTone::Info, "status")]
#[case(AlertTone::Warning, "status")]
#[case(AlertTone::Danger, "alert")]
#[serial(i18n)]
fn alert_announces_its_title_and_body_with_the_role_of_its_tone(
	#[case] tone: AlertTone,
	#[case] expected_role: &str,
) {
	within_ui(|| {
		// Arrange
		let screen = render(alert(tone, t!("Copy"), t!("Copied")));

		// Act
		let title = screen.get_by_text("Copy").text();
		let body = screen.get_by_text("Copied").text();

		// Assert
		assert_eq!(title, "Copy");
		assert_eq!(body, "Copied");
		assert!(screen.pretty().starts_with(&format!(
			"<div class=\"{}\" role=\"{expected_role}\">",
			class_value(alert_classes(tone))
		)));
	});
}

#[rstest]
#[serial(i18n)]
fn empty_state_renders_title_body_and_action() {
	within_ui(|| {
		// Arrange
		let action = button(ButtonProps::new(t!("Copy")));
		let screen = render(empty_state(
			t!("Copied"),
			t!("Skip to main content"),
			Some(action),
		));

		// Act
		let heading = screen.get_by_role(Role::Heading, "Copied");
		let action_button = screen.get_by_role(Role::Button, "Copy");

		// Assert
		assert_eq!(heading.text(), "Copied");
		assert_eq!(action_button.text(), "Copy");
		assert_eq!(
			screen.get_by_text("Skip to main content").text(),
			"Skip to main content"
		);
	});
}

#[rstest]
#[serial(i18n)]
fn code_block_renders_text_and_a_copy_button_whose_name_includes_the_title() {
	within_ui(|| {
		// Arrange
		let command = "reinhardt-cloud login".to_owned();

		// Act
		let screen = render(code_block("login", t!("Skip to main content"), command));

		// Assert
		assert_eq!(
			screen.pretty(),
			format!(
				concat!(
					"<div class=\"{code}\">\n",
					"  <div class=\"{bar}\">\n",
					"    <span>\n      Skip to main content\n    </span>\n",
					"    <button class=\"{button}\" type=\"button\" aria-controls=\"login-text\">\n",
					"      Copy\n",
					"      <span class=\"rc-visually-hidden\">\n",
					"         \n",
					"        Skip to main content\n",
					"      </span>\n",
					"    </button>\n",
					"  </div>\n",
					"  <pre class=\"rc-font-mono\">\n",
					"    <code id=\"login-text\">\n      reinhardt-cloud login\n    </code>\n",
					"  </pre>\n",
					"</div>\n",
				),
				code = class_value(CODE_BLOCK_STYLES.code()),
				bar = class_value(CODE_BLOCK_STYLES.bar()),
				button = class_value(button_classes(ButtonVariant::Quiet, ButtonSize::Small)),
			)
		);
	});
}

#[rstest]
#[serial(i18n)]
#[tokio::test]
async fn theme_toggle_offers_the_other_theme_and_flips_on_click() {
	// Arrange
	let _i18n = provide_i18n_context(i18n_context());
	let screen = render(theme_toggle);
	let before = screen
		.get_by_role(Role::Button, "Switch to dark theme")
		.text();

	// Act
	screen
		.get_by_role(Role::Button, "Switch to dark theme")
		.click();
	screen.settle().await;
	let after = screen
		.get_by_role(Role::Button, "Switch to light theme")
		.text();

	// Assert
	assert_eq!(before, "Switch to dark theme");
	assert_eq!(after, "Switch to light theme");
}

#[rstest]
#[case("light", Some(Theme::Light))]
#[case("dark", Some(Theme::Dark))]
#[case("", None)]
#[case("auto", None)]
fn theme_parses_only_the_two_explicit_values(#[case] value: &str, #[case] expected: Option<Theme>) {
	// Arrange / Act
	let parsed = Theme::parse(value);

	// Assert
	assert_eq!(parsed, expected);
}

#[rstest]
#[case(Theme::Light, Theme::Dark)]
#[case(Theme::Dark, Theme::Light)]
fn theme_toggles_to_the_other_theme_and_round_trips_through_its_attribute(
	#[case] theme: Theme,
	#[case] other: Theme,
) {
	// Arrange / Act
	let toggled = theme.toggled();

	// Assert
	assert_eq!(toggled, other);
	assert_eq!(Theme::parse(theme.as_str()), Some(theme));
}

#[rstest]
#[serial(i18n)]
fn signed_out_layout_has_skip_link_brand_theme_toggle_and_main_landmark() {
	within_ui(|| {
		// Arrange
		let content = page!({
			h1 { "Welcome" }
		});

		// Act
		let screen = render(|| signed_out_layout(content, None));
		let html = screen.pretty();

		// Assert
		assert_eq!(
			screen
				.get_by_role(Role::Link, "Skip to main content")
				.text(),
			"Skip to main content"
		);
		assert!(html.contains(&format!("href=\"#{MAIN_ID}\"")));
		assert!(html.contains(&format!("<main id=\"{MAIN_ID}\"")));
		assert!(html.contains(
			"<img src=\"/static/img/logo-mark-small.png\" alt=\"\" width=\"142\" height=\"98\">"
		));
		assert_eq!(
			screen.get_by_text("Reinhardt Cloud").text(),
			"Reinhardt Cloud"
		);
		assert_eq!(
			screen
				.get_by_role(Role::Button, "Switch to dark theme")
				.text(),
			"Switch to dark theme"
		);
		assert_eq!(
			screen.get_by_role(Role::Heading, "Welcome").text(),
			"Welcome"
		);
		assert!(!html.contains("aria-hidden"));
	});
}

#[rstest]
#[serial(i18n)]
fn signed_out_layout_hides_a_decorative_aside_from_assistive_technology() {
	within_ui(|| {
		// Arrange
		let content: Page = page!({
			h1 { "Welcome" }
		});
		let aside: Page = page!({
			p { "Preview" }
		});

		// Act
		let screen = render(|| signed_out_layout(content, Some(aside)));
		let html = screen.pretty();

		// Assert
		assert!(html.contains("aria-hidden=\"true\" inert=\"inert\""));
		assert!(screen.query_by_text("Preview").is_none());
	});
}

#[rstest]
#[serial(i18n)]
fn action_button_carries_the_design_classes() {
	within_ui(|| {
		// Arrange
		let action = use_action(|_: u32| async { Ok::<u32, String>(1) });
		let button = ActionButton::new(action, 7_u32, t!("Copy")).attr(
			"class",
			button_class(ButtonVariant::Primary, ButtonSize::Small),
		);

		// Act
		let html = button.render().render_to_string();

		// Assert
		assert_eq!(
			html,
			format!(
				"<button type=\"button\" class=\"{}\">Copy</button>",
				class_value(button_classes(ButtonVariant::Primary, ButtonSize::Small)),
			)
		);
	});
}

#[rstest]
#[serial(i18n)]
fn resource_panel_presents_each_state_with_the_design_slots() {
	within_ui(|| {
		// Arrange
		let resource = use_resource(|| async { Ok::<Vec<String>, String>(Vec::new()) }, deps![]);
		let page = ResourcePanel::new(resource)
			.loading(|| Page::text("loading"))
			.empty_if(Vec::is_empty)
			.empty(|_| empty_state(t!("Copy"), t!("Copied"), None))
			.success(|items| Page::text(format!("items:{}", items.len())))
			.error(|message| alert(AlertTone::Danger, t!("Copy"), Page::text(message.clone())))
			.render();

		// Act
		resource.set(ResourceState::Success(Vec::new()));
		let empty = page.render_to_string();
		resource.set(ResourceState::Success(vec!["one".to_owned()]));
		let success = page.render_to_string();
		resource.set(ResourceState::Error("boom".to_owned()));
		let error = page.render_to_string();

		// Assert
		assert_eq!(
			empty,
			empty_state(t!("Copy"), t!("Copied"), None).render_to_string()
		);
		assert_eq!(success, "items:1");
		assert_eq!(
			error,
			alert(AlertTone::Danger, t!("Copy"), Page::text("boom")).render_to_string()
		);
	});
}

#[rstest]
#[serial(i18n)]
#[tokio::test]
async fn failed_action_is_presented_with_the_alert_next_to_the_styled_button() {
	// Arrange
	let _i18n = provide_i18n_context(i18n_context());
	let screen = render(|| {
		let action = use_action(|_: ()| async { Err::<(), String>("boom".to_owned()) });
		let trigger = ActionButton::new(action, (), t!("Copy"))
			.attr(
				"class",
				button_class(ButtonVariant::Primary, ButtonSize::Regular),
			)
			.render();
		let result = ActionResultPanel::new(action)
			.idle(|| Page::text("idle"))
			.error(|message| alert(AlertTone::Danger, t!("Copied"), Page::text(message.clone())))
			.render();
		page!({
			{
				trigger
			}
			{ result }
		})
	});
	let before = screen.pretty();

	// Act
	screen.get_by_role(Role::Button, "Copy").click();
	screen.settle().await;
	let after = screen.pretty();

	// Assert
	assert!(before.ends_with("idle\n"));
	assert_eq!(
		after,
		format!(
			concat!(
				"<button type=\"button\" class=\"{button}\">\n  Copy\n</button>\n",
				"<div class=\"{alert}\" role=\"alert\">\n",
				"  <p class=\"{title}\">\n    Copied\n  </p>\n",
				"  <p class=\"{body}\">\n    boom\n  </p>\n",
				"</div>\n",
			),
			button = class_value(button_classes(ButtonVariant::Primary, ButtonSize::Regular)),
			alert = class_value(alert_classes(AlertTone::Danger)),
			title = class_value(ALERT_STYLES.title()),
			body = class_value(ALERT_STYLES.body()),
		)
	);
}

#[rstest]
#[serial(i18n)]
fn dialog_view_is_labelled_by_its_title_and_starts_closed() {
	within_ui(|| {
		// Arrange
		let props = DialogProps::new(
			"confirm",
			t!("Copy"),
			Page::text("Body text"),
			button(ButtonProps::new(t!("Copied"))),
		);

		// Act
		let screen = render(dialog_view(props));

		// Assert
		assert_eq!(
			screen.pretty(),
			format!(
				concat!(
					"<dialog id=\"confirm\" class=\"{dialog}\" aria-labelledby=\"confirm-title\">\n",
					"  <h2 id=\"confirm-title\" class=\"{title}\">\n    Copy\n  </h2>\n",
					"  <div class=\"{body}\">\n    Body text\n  </div>\n",
					"  <div class=\"{actions}\">\n",
					"    <button class=\"{button}\" type=\"button\">\n      Copied\n    </button>\n",
					"  </div>\n",
					"</dialog>\n",
				),
				dialog = class_value(dialog_classes()),
				title = class_value(DIALOG_STYLES.title()),
				body = class_value(DIALOG_STYLES.body()),
				actions = class_value(DIALOG_STYLES.actions()),
				button = class_value(button_classes(
					ButtonVariant::Secondary,
					ButtonSize::Regular
				)),
			)
		);
	});
}

#[rstest]
#[serial(i18n)]
fn open_dialog_mounts_nothing_on_the_server_target() {
	within_ui(|| {
		// Arrange
		let props = DialogProps::new("confirm", t!("Copy"), Page::text("Body"), Page::empty());

		// Act
		let open = open_dialog(props, || {}).expect("portals accept any view on the server target");

		// Assert
		assert!(!open.is_open());
	});
}

#[rstest]
fn form_classes_compose_the_design_for_client_forms() {
	// Arrange / Act
	let classes = form_classes();

	// Assert
	assert_eq!(classes.form, class_value(FORM_STYLES.form()));
	assert_eq!(classes.field, class_value(FORM_STYLES.field()));
	assert_eq!(classes.input, class_value(FORM_STYLES.input()));
	assert_eq!(classes.label, class_value(FORM_STYLES.label()));
	assert_eq!(classes.help, class_value(FORM_STYLES.help()));
	assert_eq!(classes.error, class_value(FORM_STYLES.error()));
	assert_eq!(classes.summary, class_value(FORM_STYLES.summary()));
	assert_eq!(
		classes.select,
		format!(
			"{} {} rc-select-chevron",
			class_value(FORM_STYLES.input()),
			class_value(FORM_STYLES.select())
		)
	);
	assert_eq!(
		classes.submit,
		class_value(button_classes(ButtonVariant::Primary, ButtonSize::Regular))
	);
}

#[rstest]
#[serial(i18n)]
fn signed_out_layout_renders_a_router_outlet_as_its_content() {
	within_ui(|| {
		// Arrange
		let outlet = Outlet::inline(page!({
			h1 { "Welcome" }
		}));

		// Act
		let screen = render(|| signed_out_layout(outlet, None));

		// Assert
		assert_eq!(
			screen.get_by_role(Role::Heading, "Welcome").text(),
			"Welcome"
		);
		assert!(screen.pretty().contains(&format!("<main id=\"{MAIN_ID}\"")));
	});
}

#[rstest]
#[case::two_words("Mina Okafor", "mokafor", "MO")]
#[case::one_word("riley", "rchen", "R")]
#[case::more_than_two_words("Riley Chen Park", "rpark", "RC")]
#[case::surrounding_space("  Riley   Chen ", "rchen", "RC")]
#[case::falls_back_to_the_login("", "octocat", "O")]
#[case::blank_name_falls_back_to_the_login("   ", "octocat", "O")]
#[case::accented_letters("élodie durand", "edurand", "ÉD")]
#[case::nothing_at_all("", "", "")]
fn initials_come_from_the_name_and_fall_back_to_the_login(
	#[case] name: &str,
	#[case] login: &str,
	#[case] expected: &str,
) {
	// Arrange / Act
	let result = initials(name, login);

	// Assert
	assert_eq!(result, expected);
}

#[rstest]
#[serial(i18n)]
fn avatar_shows_its_initials_and_hides_from_assistive_technology() {
	within_ui(|| {
		// Arrange
		let large = class_value(avatar_classes(AvatarSize::Large));
		let regular = class_value(avatar_classes(AvatarSize::Regular));

		// Act
		let html = avatar("MO".to_owned(), AvatarSize::Large).render_to_string();

		// Assert
		assert_eq!(
			html,
			format!("<span class=\"{large}\" aria-hidden=\"true\">MO</span>")
		);
		assert!(large.contains(&class_value(AVATAR_STYLES.large())));
		assert!(!regular.contains(&class_value(AVATAR_STYLES.large())));
	});
}

fn frame_with(switcher: Option<Page>, overview_is_current: bool, user_menu: Page) -> SignedInFrame {
	SignedInFrame {
		home_href: "/".to_owned(),
		switcher,
		nav: vec![
			NavItem {
				label: t!("Overview"),
				href: "/".to_owned(),
				current: overview_is_current,
			},
			NavItem {
				label: t!("Account"),
				href: "/account/".to_owned(),
				current: !overview_is_current,
			},
		],
		user_menu,
	}
}

#[rstest]
#[serial(i18n)]
fn signed_in_layout_has_the_landmarks_the_brand_the_navigation_and_the_theme_toggle() {
	within_ui(|| {
		// Arrange
		let content = page!({
			h1 { "Overview" }
		});
		let user_menu: Page = page!({
			p { "user menu slot" }
		});

		// Act
		let screen = render(|| signed_in_layout(frame_with(None, true, user_menu), content));
		let html = screen.pretty();

		// Assert
		assert_eq!(
			screen
				.get_by_role(Role::Link, "Skip to main content")
				.text(),
			"Skip to main content"
		);
		assert!(html.contains(&format!("href=\"#{MAIN_ID}\"")));
		assert!(html.contains(&format!("<main id=\"{MAIN_ID}\"")));
		assert_eq!(
			screen.get_by_role(Role::Navigation, "Primary").tag_name(),
			"nav"
		);
		assert!(html.contains("aria-label=\"Sidebar\""));
		assert!(html.contains(
			"<img src=\"/static/img/logo-mark-small.png\" alt=\"\" width=\"142\" height=\"98\">"
		));
		assert_eq!(
			screen.get_by_role(Role::Heading, "Overview").text(),
			"Overview"
		);
		assert_eq!(
			screen.get_by_text("user menu slot").text(),
			"user menu slot"
		);
		assert_eq!(
			screen
				.get_by_role(Role::Button, "Switch to dark theme")
				.text(),
			"Switch to dark theme"
		);
	});
}

#[rstest]
#[case::overview(true, "/", "/account/")]
#[case::account(false, "/account/", "/")]
#[serial(i18n)]
fn signed_in_layout_marks_exactly_the_current_page_in_the_navigation(
	#[case] overview_is_current: bool,
	#[case] marked_href: &str,
	#[case] unmarked_href: &str,
) {
	within_ui(|| {
		// Arrange
		let content: Page = page!({
			p { "content" }
		});

		// Act
		let screen = render(|| {
			signed_in_layout(
				frame_with(None, overview_is_current, Page::empty()),
				content,
			)
		});
		let html = screen.pretty();

		// Assert
		let marked_lines: Vec<&str> = html
			.lines()
			.filter(|line| line.contains("aria-current"))
			.collect();
		assert_eq!(marked_lines.len(), 1, "{html}");
		assert!(
			marked_lines[0].contains(&format!("href=\"{marked_href}\" aria-current=\"page\"")),
			"{html}"
		);
		let unmarked = html
			.lines()
			.find(|line| {
				line.contains(&format!("href=\"{unmarked_href}\"")) && line.contains("nav_link")
			})
			.expect("the other link is in the navigation");
		assert!(!unmarked.contains("aria-current"), "{unmarked}");
	});
}

#[rstest]
#[serial(i18n)]
fn signed_in_layout_renders_nothing_for_an_absent_organization_switcher() {
	within_ui(|| {
		// Arrange
		let content: Page = page!({
			p { "content" }
		});
		let switcher: Page = page!({
			p { "switcher slot" }
		});

		// Act
		let without =
			render(|| signed_in_layout(frame_with(None, true, Page::empty()), content.clone()));
		let with =
			render(|| signed_in_layout(frame_with(Some(switcher), true, Page::empty()), content));

		// Assert
		assert!(without.query_by_text("switcher slot").is_none());
		assert_eq!(with.get_by_text("switcher slot").text(), "switcher slot");
		let html = without.pretty();
		let after_brand = &html[html.find("Reinhardt Cloud").expect("the brand is rendered")..];
		let brand_end = after_brand.find("</a>").expect("the brand link closes") + "</a>".len();
		let nav_start = after_brand.find("<nav").expect("the navigation opens");
		assert_eq!(
			after_brand[brand_end..nav_start].trim(),
			"",
			"nothing sits between the brand and the navigation"
		);
	});
}

#[rstest]
#[serial(i18n)]
fn menu_is_a_disclosure_with_its_toggle_and_entries() {
	within_ui(|| {
		// Arrange
		let who = person(
			avatar("MO".to_owned(), AvatarSize::Regular),
			t!("User menu"),
			"Mina Okafor".to_owned(),
			"@mokafor".to_owned(),
		);
		let entries = vec![menu_link("/account/".to_owned(), t!("Account"))];

		// Act
		let screen = render(|| menu(who, entries));
		let html = screen.pretty();

		// Assert
		assert_eq!(html.matches("<details").count(), 1);
		assert!(html.contains("data-menu=\"true\""));
		assert_eq!(html.matches("<summary").count(), 1);
		assert_eq!(screen.get_by_text("Mina Okafor").text(), "Mina Okafor");
		assert_eq!(screen.get_by_text("@mokafor").text(), "@mokafor");
		assert_eq!(screen.get_by_text("User menu").text(), "User menu");
		assert_eq!(screen.get_by_role(Role::Link, "Account").text(), "Account");
		assert!(html.contains("href=\"/account/\""));
	});
}

#[rstest]
#[serial(i18n)]
fn menu_item_class_is_the_class_menu_links_carry() {
	within_ui(|| {
		// Arrange
		let link = menu_link("/account/".to_owned(), t!("Account"));

		// Act
		let html = link.render_to_string();

		// Assert
		assert_eq!(
			html,
			format!(
				"<a class=\"{}\" href=\"/account/\">Account</a>",
				menu_item_class()
			)
		);
	});
}

#[rstest]
#[serial(i18n)]
fn page_head_and_section_label_their_content() {
	within_ui(|| {
		// Arrange
		let body: Page = page!({
			p { "facts" }
		});

		// Act
		let screen = render(|| {
			Page::Fragment(vec![
				page_head(
					t!("Account"),
					t!("The GitHub profile this Control Plane knows you by."),
				),
				page_section("account-profile", t!("Profile"), body),
			])
		});
		let html = screen.pretty();

		// Assert
		assert_eq!(
			screen.get_by_role(Role::Heading, "Account").tag_name(),
			"h1"
		);
		assert_eq!(
			screen.get_by_role(Role::Heading, "Profile").tag_name(),
			"h2"
		);
		assert!(html.contains("aria-labelledby=\"account-profile-title\""));
		assert!(html.contains("id=\"account-profile-title\""));
		assert_eq!(screen.get_by_text("facts").text(), "facts");
	});
}
