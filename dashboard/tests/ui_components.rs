//! Native tests for the design-system components.

use std::borrow::Cow;
use std::cell::Cell;
use std::rc::Rc;

use cloud_control_plane::i18n::i18n_context;
use cloud_control_plane::ui::alert::{AlertTone, alert, alert_classes};
use cloud_control_plane::ui::badge::{BadgeStatus, badge, badge_classes, chip};
use cloud_control_plane::ui::button::{
	ButtonProps, ButtonSize, ButtonVariant, button, button_classes, link_button,
};
use cloud_control_plane::ui::code_block::code_block;
use cloud_control_plane::ui::dialog::{DIALOG_STYLES, DialogProps, dialog, dialog_classes};
use cloud_control_plane::ui::empty_state::empty_state;
use cloud_control_plane::ui::field::{
	FIELD_STYLES, SelectFieldProps, SelectOption, TextFieldProps, control_classes, select_field,
	text_field,
};
use cloud_control_plane::ui::layout::signed_out::{MAIN_ID, signed_out_layout};
use cloud_control_plane::ui::table::{TABLE_STYLES, TableColumn, data_table};
use cloud_control_plane::ui::theme::{Theme, theme_toggle};
use reinhardt::pages::component::Page;
use reinhardt::pages::event::ClickEvent;
use reinhardt::pages::i18n::provide_i18n_context;
use reinhardt::pages::reactive::{ReactiveScope, Signal};
use reinhardt::pages::testing::component::{Role, render};
use reinhardt::pages::{Callback, page, t};
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
#[serial(i18n)]
fn alert_announces_its_title_and_body() {
	within_ui(|| {
		// Arrange
		let screen = render(alert(AlertTone::Warning, t!("Copy"), t!("Copied")));

		// Act
		let title = screen.get_by_text("Copy").text();
		let body = screen.get_by_text("Copied").text();

		// Assert
		assert_eq!(title, "Copy");
		assert_eq!(body, "Copied");
		assert!(screen.pretty().starts_with(&format!(
			"<div class=\"{}\" role=\"alert\">",
			class_value(alert_classes(AlertTone::Warning))
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
fn text_field_pairs_label_and_input_and_binds_the_value() {
	within_ui(|| {
		// Arrange
		let value = Signal::new(String::new());
		let props = TextFieldProps::new("display-name", t!("Copy"), value);
		let screen = render(text_field(props));

		// Act
		screen.get_by_label("Copy").input("Ada");

		// Assert
		assert_eq!(value.get_untracked(), "Ada");
	});
}

#[rstest]
#[serial(i18n)]
fn text_field_marks_the_control_invalid_and_describes_it() {
	within_ui(|| {
		// Arrange
		let value = Signal::new(String::new());
		let props = TextFieldProps::new("display-name", t!("Copy"), value)
			.hint(t!("Skip to main content"))
			.error(t!("Copied"));

		// Act
		let screen = render(text_field(props));

		// Assert
		assert_eq!(
			screen.pretty(),
			format!(
				concat!(
					"<div class=\"{field}\">\n",
					"  <label class=\"{label}\" for=\"display-name\">\n    Copy\n  </label>\n",
					"  <input id=\"display-name\" type=\"text\" class=\"{control}\" placeholder=\"\" aria-invalid=\"true\" aria-describedby=\"display-name-hint display-name-error\" value=\"\">\n",
					"  <p id=\"display-name-hint\" class=\"{hint}\">\n    Skip to main content\n  </p>\n",
					"  <p id=\"display-name-error\" class=\"{error}\">\n    Copied\n  </p>\n",
					"</div>\n",
				),
				field = class_value(FIELD_STYLES.field()),
				label = class_value(FIELD_STYLES.label()),
				control = class_value(control_classes(false, false)),
				hint = class_value(FIELD_STYLES.hint()),
				error = class_value(FIELD_STYLES.error()),
			)
		);
	});
}

#[rstest]
#[serial(i18n)]
fn select_field_lists_options_and_binds_the_selection() {
	within_ui(|| {
		// Arrange
		let value = Signal::new("viewer".to_owned());
		let options = vec![
			SelectOption {
				value: "viewer".to_owned(),
				label: t!("Copy"),
			},
			SelectOption {
				value: "owner".to_owned(),
				label: t!("Copied"),
			},
		];
		let props = SelectFieldProps::new("role", t!("Skip to main content"), value, options);
		let screen = render(select_field(props));

		// Act
		screen.get_by_label("Skip to main content").change("owner");

		// Assert
		assert_eq!(value.get_untracked(), "owner");
		assert_eq!(screen.get_by_text("Copy").text(), "Copy");
		assert_eq!(screen.get_by_text("Copied").text(), "Copied");
	});
}

#[rstest]
#[serial(i18n)]
fn data_table_renders_caption_headers_and_row_headers() {
	within_ui(|| {
		// Arrange
		let columns = vec![
			TableColumn::new(t!("Copy")),
			TableColumn::new(t!("Copied")).align_end(),
		];
		let rows = vec![vec![Page::text("alice"), Page::text("owner")]];

		// Act
		let screen = render(data_table(t!("Skip to main content"), columns, rows));

		// Assert
		assert_eq!(
			screen.pretty(),
			format!(
				concat!(
					"<div class=\"{wrap}\">\n",
					"  <table class=\"{table}\">\n",
					"    <caption class=\"rc-visually-hidden\">\n      Skip to main content\n    </caption>\n",
					"    <thead>\n      <tr>\n",
					"        <th scope=\"col\" class=\"\">\n          Copy\n        </th>\n",
					"        <th scope=\"col\" class=\"{end}\">\n          Copied\n        </th>\n",
					"      </tr>\n    </thead>\n",
					"    <tbody>\n      <tr>\n",
					"        <th scope=\"row\" class=\"\">\n          alice\n        </th>\n",
					"        <td class=\"{end}\">\n          owner\n        </td>\n",
					"      </tr>\n    </tbody>\n",
					"  </table>\n</div>\n",
				),
				wrap = class_value(TABLE_STYLES.wrap()),
				table = class_value(TABLE_STYLES.table()),
				end = class_value(TABLE_STYLES.end()),
			)
		);
	});
}

#[rstest]
#[serial(i18n)]
fn dialog_is_labelled_by_its_title_and_starts_closed() {
	within_ui(|| {
		// Arrange
		let props = DialogProps::new(
			"confirm",
			t!("Copy"),
			Page::text("Body text"),
			button(ButtonProps::new(t!("Copied"))),
		);

		// Act
		let screen = render(dialog(props));

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
fn code_block_shows_the_text_and_a_labelled_copy_button() {
	within_ui(|| {
		// Arrange
		let command = "reinhardt-cloud login".to_owned();

		// Act
		let screen = render(code_block("login", t!("Skip to main content"), command));

		// Assert
		assert_eq!(
			screen.get_by_text("reinhardt-cloud login").text(),
			"reinhardt-cloud login"
		);
		assert_eq!(
			screen.get_by_text("Skip to main content").text(),
			"Skip to main content"
		);
		let copy = screen.get_by_role(Role::Button, "Copy");
		assert_eq!(copy.text(), "Copy");
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
