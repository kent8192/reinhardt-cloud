//! Design styles for `form!` / `ClientForm` rendering.
//!
//! `reinhardt-pages` checked: `form!` with `client_form:` and `mutation:` (see
//! `reinhardt-pages/docs/client_forms.md`) renders labels, controls, help and
//! error text, a validation summary, and a pending-aware submit button, and its
//! `styling:` / `customize:` entries take class *expressions*. That output can
//! carry the design's typed class tokens directly, so there is no field or
//! select component here: a form passes [`form_classes`] to `styling:` (and
//! `select` / `submit` to the matching `customize:` / `submit:` entries).

use reinhardt::pages::{ClassList, style_def};

use crate::components::button::{ButtonSize, ButtonVariant, button_class};

#[style_def]
pub static FORM_STYLES: FormStyles = style! {
	globals {
		space_1: Length;
		space_3: Length;
		space_4: Length;
		space_6: Length;
		control_height: Length;
		radius_md: Length;
		surface_raised: Color;
		border_default: Color;
		border_strong: Color;
		ink_strong: Color;
		ink_subtle: Color;
		text_sm: Length;
		weight_medium: Number;
		weight_semibold: Number;
		status_danger_fg: Color;
	}
	.form {
		display: flex;
		flex-direction: column;
		gap: globals.space_4;
	}
	.field {
		display: flex;
		flex-direction: column;
		gap: globals.space_1;
	}
	.label {
		color: globals.ink_strong;
		font-weight: globals.weight_medium;
	}
	.help {
		color: globals.ink_subtle;
		font-size: globals.text_sm;
	}
	.error {
		color: globals.status_danger_fg;
		font-size: globals.text_sm;
	}
	.summary {
		padding: (globals.space_3, globals.space_4);
		border: (1px, solid, globals.border_default);
		border-left: (3px, solid, globals.status_danger_fg);
		border-radius: globals.radius_md;
		background: globals.surface_raised;
		color: globals.ink_strong;
		font-weight: globals.weight_semibold;
	}
	.input {
		width: 100%;
		min-height: globals.control_height;
		padding: (0, globals.space_3);
		background: globals.surface_raised;
		border: (1px, solid, globals.border_strong);
		border-radius: globals.radius_md;
		color: globals.ink_strong;
		&::placeholder {
			color: globals.ink_subtle;
		}
		&[aria-invalid="true"] {
			border-color: globals.status_danger_fg;
		}
		&:disabled {
			opacity: 0.6;
			cursor: not-allowed;
		}
	}
	.select {
		padding-right: globals.space_6;
	}
};

/// Class values for the `styling:`, `customize:`, and `submit:` entries of a
/// `form!` with `client_form:`.
///
/// Every field is an owned string because those entries evaluate to strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormClasses {
	/// `styling: { class: .. }`.
	pub form: String,
	/// `styling: { field_class: .. }`.
	pub field: String,
	/// `styling: { input_class: .. }`.
	pub input: String,
	/// `customize: { field: { class: .. } }` for `Select` controls.
	pub select: String,
	/// `styling: { label_class: .. }`.
	pub label: String,
	/// `styling: { help_class: .. }`.
	pub help: String,
	/// `styling: { error_class: .. }`.
	pub error: String,
	/// `styling: { summary_class: .. }`.
	pub summary: String,
	/// `submit: { class: .. }`.
	pub submit: String,
}

fn owned(classes: ClassList) -> String {
	std::borrow::Cow::from(classes).into_owned()
}

/// Returns the design's class values for a client form.
pub fn form_classes() -> FormClasses {
	FormClasses {
		form: owned(FORM_STYLES.form() + ""),
		field: owned(FORM_STYLES.field() + ""),
		input: owned(FORM_STYLES.input() + ""),
		select: owned(FORM_STYLES.input() + FORM_STYLES.select() + "rc-select-chevron"),
		label: owned(FORM_STYLES.label() + ""),
		help: owned(FORM_STYLES.help() + ""),
		error: owned(FORM_STYLES.error() + ""),
		summary: owned(FORM_STYLES.summary() + ""),
		submit: button_class(ButtonVariant::Primary, ButtonSize::Regular),
	}
}
