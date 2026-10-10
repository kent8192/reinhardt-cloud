//! Button styles, plus a plain button and a link styled as a button.
//!
//! `reinhardt-pages` checked: `ui::ActionButton` (dispatches an `Action`) and
//! `ui::FormActionButton` (submits a form `FormAction`) cover action buttons;
//! style them with `.attr("class", button_class(..))`. Neither takes an
//! arbitrary click handler, and neither renders an anchor, so `button` (theme
//! toggle, copy, dialog controls) and `link_button` (navigation styled as a
//! button) stay as thin components here.

use std::borrow::Cow;

use reinhardt::pages::component::Page;
use reinhardt::pages::event::ClickEvent;
use reinhardt::pages::{Callback, ClassList, TranslatedText, page, style_def};

#[style_def]
pub static BUTTON_STYLES: ButtonStyles = style! {
	globals {
		space_2: Length;
		space_3: Length;
		space_4: Length;
		space_5: Length;
		control_height: Length;
		radius_md: Length;
		surface_raised: Color;
		surface_hover: Color;
		surface_inverse: Color;
		border_strong: Color;
		ink_strong: Color;
		ink_muted: Color;
		ink_inverse: Color;
		accent_fill: Color;
		accent_fill_hover: Color;
		accent_on_fill: Color;
		status_danger_fg: Color;
		status_danger_bg: Color;
		text_sm: Length;
		text_lg: Length;
		weight_medium: Number;
		weight_semibold: Number;
		border_width: Length;
		control_height_lg: Length;
		control_height_sm: Length;
	}
	.btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: globals.space_2;
		min-height: globals.control_height;
		padding: (0, globals.space_4);
		background: globals.surface_raised;
		border: (globals.border_width, solid, globals.border_strong);
		border-radius: globals.radius_md;
		color: globals.ink_strong;
		font-weight: globals.weight_medium;
		line-height: 1;
		text-decoration: none;
		white-space: nowrap;
		cursor: pointer;
		&:hover {
			background: globals.surface_hover;
		}
		&:disabled, &[aria-disabled="true"] {
			opacity: 0.5;
			cursor: not-allowed;
		}
	}
	.primary {
		background: globals.accent_fill;
		border-color: transparent;
		color: globals.accent_on_fill;
		font-weight: globals.weight_semibold;
		&:hover {
			background: globals.accent_fill_hover;
		}
	}
	.quiet {
		background: transparent;
		border-color: transparent;
		color: globals.ink_muted;
		&:hover {
			background: globals.surface_hover;
			color: globals.ink_strong;
		}
	}
	.danger {
		background: transparent;
		border-color: globals.status_danger_fg;
		color: globals.status_danger_fg;
		&:hover {
			background: globals.status_danger_bg;
		}
	}
	.quiet_danger {
		color: globals.status_danger_fg;
		&:hover {
			background: globals.status_danger_bg;
			color: globals.status_danger_fg;
		}
	}
	.github {
		background: globals.surface_inverse;
		border-color: transparent;
		color: globals.ink_inverse;
		min-height: globals.control_height_lg;
		padding: (0, globals.space_5);
		font-size: globals.text_lg;
		font-weight: globals.weight_semibold;
		&:hover {
			background: globals.surface_inverse;
			opacity: 0.88;
		}
	}
	.small {
		min-height: globals.control_height_sm;
		padding: (0, globals.space_3);
		font-size: globals.text_sm;
	}
};

/// Visual emphasis of a button.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonVariant {
	/// Bordered neutral button for ordinary actions.
	#[default]
	Secondary,
	/// Gold-filled button for the single main action of a view.
	Primary,
	/// Borderless button for low-emphasis actions.
	Quiet,
	/// Outlined button for destructive actions.
	Danger,
	/// Borderless destructive button for row actions.
	QuietDanger,
	/// Inverse button for "Continue with GitHub".
	Github,
}

/// Size of a button.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonSize {
	/// Default control height.
	#[default]
	Regular,
	/// Compact height for table rows and toolbars.
	Small,
}

/// Returns the class list for a button of the given variant and size.
pub fn button_classes(variant: ButtonVariant, size: ButtonSize) -> ClassList {
	let base = BUTTON_STYLES.btn() + "";
	let styled = match variant {
		ButtonVariant::Secondary => base,
		ButtonVariant::Primary => base + BUTTON_STYLES.primary(),
		ButtonVariant::Quiet => base + BUTTON_STYLES.quiet(),
		ButtonVariant::Danger => base + BUTTON_STYLES.danger(),
		ButtonVariant::QuietDanger => base + BUTTON_STYLES.quiet() + BUTTON_STYLES.quiet_danger(),
		ButtonVariant::Github => base + BUTTON_STYLES.github(),
	};
	match size {
		ButtonSize::Regular => styled,
		ButtonSize::Small => styled + BUTTON_STYLES.small(),
	}
}

/// Returns the `class` attribute value of a button as an owned string.
///
/// Pass it to `ActionButton::attr("class", ..)` or
/// `FormActionButton::attr("class", ..)`, which accept only string attributes.
pub fn button_class(variant: ButtonVariant, size: ButtonSize) -> String {
	Cow::from(button_classes(variant, size)).into_owned()
}

/// Properties of a [`button`].
#[derive(Clone)]
pub struct ButtonProps {
	label: TranslatedText,
	variant: ButtonVariant,
	size: ButtonSize,
	disabled: bool,
	submit: bool,
	on_click: Option<Callback<ClickEvent>>,
}

impl ButtonProps {
	/// Creates properties for a secondary, regular-size `type="button"` button.
	pub fn new(label: TranslatedText) -> Self {
		Self {
			label,
			variant: ButtonVariant::default(),
			size: ButtonSize::default(),
			disabled: false,
			submit: false,
			on_click: None,
		}
	}

	/// Sets the visual emphasis.
	pub fn variant(mut self, variant: ButtonVariant) -> Self {
		self.variant = variant;
		self
	}

	/// Sets the size.
	pub fn size(mut self, size: ButtonSize) -> Self {
		self.size = size;
		self
	}

	/// Disables the button.
	pub fn disabled(mut self, disabled: bool) -> Self {
		self.disabled = disabled;
		self
	}

	/// Makes the button submit its form instead of acting as a plain button.
	pub fn submit(mut self) -> Self {
		self.submit = true;
		self
	}

	/// Sets the click handler.
	pub fn on_click(mut self, handler: Callback<ClickEvent>) -> Self {
		self.on_click = Some(handler);
		self
	}
}

/// Renders a `<button>`.
pub fn button(props: ButtonProps) -> Page {
	let ButtonProps {
		label,
		variant,
		size,
		disabled,
		submit,
		on_click,
	} = props;
	let classes = button_classes(variant, size);
	let kind = if submit { "submit" } else { "button" };
	page!({
		button {
			class: classes,
			type: kind,
			disabled: disabled,
			@click: move |event: ClickEvent| {
				if let Some(handler) = on_click {
					handler.call(event);
				}
			},
			{ label }
		}
	})
}

/// Renders an `<a>` styled as a button, for navigation to `href`.
///
/// `href` must come from a route reverse helper.
pub fn link_button(
	href: String,
	label: TranslatedText,
	variant: ButtonVariant,
	size: ButtonSize,
) -> Page {
	let classes = button_classes(variant, size);
	page!({
		a {
			class: classes,
			href: href,
			{ label }
		}
	})
}

/// Renders an `<a>` styled as a button, for navigation to a URL the server
/// answers itself (a redirect to another site, a download).
///
/// `rel="external"` keeps the client's link interception from routing the
/// click through the client router, which has no route for it and would drop
/// the navigation.
pub fn external_link_button(
	href: String,
	label: TranslatedText,
	variant: ButtonVariant,
	size: ButtonSize,
) -> Page {
	let classes = button_classes(variant, size);
	page!({
		a {
			class: classes,
			href: href,
			rel: "external",
			{ label }
		}
	})
}
