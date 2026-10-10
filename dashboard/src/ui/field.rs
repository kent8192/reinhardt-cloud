//! Labelled text input and select.

use reinhardt::pages::component::Page;
use reinhardt::pages::reactive::Signal;
use reinhardt::pages::{ClassList, TranslatedText, page, style_def};

#[style_def]
pub static FIELD_STYLES: FieldStyles = style! {
	globals {
		space_1: Length;
		space_3: Length;
		space_6: Length;
		control_height: Length;
		radius_md: Length;
		surface_raised: Color;
		border_strong: Color;
		ink_strong: Color;
		ink_subtle: Color;
		text_sm: Length;
		weight_medium: Number;
		status_danger_fg: Color;
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
	.hint {
		color: globals.ink_subtle;
		font-size: globals.text_sm;
	}
	.error {
		color: globals.status_danger_fg;
		font-size: globals.text_sm;
	}
	.control {
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

/// Returns the class list of a text input or select control.
pub fn control_classes(select: bool, monospace: bool) -> ClassList {
	let base = FIELD_STYLES.control() + "";
	let base = if select {
		base + FIELD_STYLES.select() + "rc-select-chevron"
	} else {
		base
	};
	if monospace {
		base + "rc-font-mono"
	} else {
		base
	}
}

/// Shared properties of a labelled control.
#[derive(Clone)]
struct FieldChrome {
	id: String,
	label: TranslatedText,
	hint: Option<TranslatedText>,
	error: Option<TranslatedText>,
	disabled: bool,
}

impl FieldChrome {
	fn new(id: String, label: TranslatedText) -> Self {
		Self {
			id,
			label,
			hint: None,
			error: None,
			disabled: false,
		}
	}

	fn hint_id(&self) -> String {
		format!("{}-hint", self.id)
	}

	fn error_id(&self) -> String {
		format!("{}-error", self.id)
	}

	/// Space-separated IDs of the notes that describe the control.
	fn described_by(&self) -> String {
		let mut ids = Vec::new();
		if self.hint.is_some() {
			ids.push(self.hint_id());
		}
		if self.error.is_some() {
			ids.push(self.error_id());
		}
		ids.join(" ")
	}

	fn invalid(&self) -> &'static str {
		if self.error.is_some() {
			"true"
		} else {
			"false"
		}
	}
}

fn note(id: String, class: reinhardt::pages::ClassToken, text: Option<TranslatedText>) -> Page {
	match text {
		Some(text) => page!({
			p {
				id: id,
				class: class,
				{ text }
			}
		}),
		None => Page::empty(),
	}
}

fn hint_note(chrome: &FieldChrome) -> Page {
	note(chrome.hint_id(), FIELD_STYLES.hint(), chrome.hint.clone())
}

fn error_note(chrome: &FieldChrome) -> Page {
	note(
		chrome.error_id(),
		FIELD_STYLES.error(),
		chrome.error.clone(),
	)
}

/// Properties of a [`text_field`].
#[derive(Clone)]
pub struct TextFieldProps {
	chrome: FieldChrome,
	value: Signal<String>,
	placeholder: Option<TranslatedText>,
	monospace: bool,
}

impl TextFieldProps {
	/// Creates properties for a text input bound to `value`.
	///
	/// `id` must be unique in the document; it pairs the label with the input.
	pub fn new(id: impl Into<String>, label: TranslatedText, value: Signal<String>) -> Self {
		Self {
			chrome: FieldChrome::new(id.into(), label),
			value,
			placeholder: None,
			monospace: false,
		}
	}

	/// Sets the placeholder, resolved in the active locale at render time.
	pub fn placeholder(mut self, placeholder: TranslatedText) -> Self {
		self.placeholder = Some(placeholder);
		self
	}

	/// Sets the hint shown under the control.
	pub fn hint(mut self, hint: TranslatedText) -> Self {
		self.chrome.hint = Some(hint);
		self
	}

	/// Marks the control invalid and shows `error` under it.
	pub fn error(mut self, error: TranslatedText) -> Self {
		self.chrome.error = Some(error);
		self
	}

	/// Renders the value in the monospace face, for identifiers and URLs.
	pub fn monospace(mut self) -> Self {
		self.monospace = true;
		self
	}

	/// Disables the control.
	pub fn disabled(mut self, disabled: bool) -> Self {
		self.chrome.disabled = disabled;
		self
	}
}

/// Renders a labelled text input.
pub fn text_field(props: TextFieldProps) -> Page {
	let TextFieldProps {
		chrome,
		value,
		placeholder,
		monospace,
	} = props;
	let classes = control_classes(false, monospace);
	let placeholder = placeholder
		.map(|text| text.render_string())
		.unwrap_or_default();
	let id = chrome.id.clone();
	let label = chrome.label.clone();
	let disabled = chrome.disabled;
	let invalid = chrome.invalid();
	let described_by = chrome.described_by();
	let hint = hint_note(&chrome);
	let error = error_note(&chrome);
	page!({
		div {
			class: FIELD_STYLES.field(),
			label {
				class: FIELD_STYLES.label(),
				for: id.clone(),
				{ label }
			}
			input {
				id: id,
				type: "text",
				class: classes,
				bind: value,
				placeholder: placeholder,
				disabled: disabled,
				aria_invalid: invalid,
				aria_describedby: described_by,
			}
			{ hint }
			{ error }
		}
	})
}

/// One choice of a [`select_field`].
#[derive(Clone)]
pub struct SelectOption {
	/// Submitted value.
	pub value: String,
	/// Visible label.
	pub label: TranslatedText,
}

/// Properties of a [`select_field`].
#[derive(Clone)]
pub struct SelectFieldProps {
	chrome: FieldChrome,
	value: Signal<String>,
	options: Vec<SelectOption>,
}

impl SelectFieldProps {
	/// Creates properties for a select bound to `value`.
	///
	/// `id` must be unique in the document; it pairs the label with the select.
	pub fn new(
		id: impl Into<String>,
		label: TranslatedText,
		value: Signal<String>,
		options: Vec<SelectOption>,
	) -> Self {
		Self {
			chrome: FieldChrome::new(id.into(), label),
			value,
			options,
		}
	}

	/// Sets the hint shown under the control.
	pub fn hint(mut self, hint: TranslatedText) -> Self {
		self.chrome.hint = Some(hint);
		self
	}

	/// Marks the control invalid and shows `error` under it.
	pub fn error(mut self, error: TranslatedText) -> Self {
		self.chrome.error = Some(error);
		self
	}

	/// Disables the control.
	pub fn disabled(mut self, disabled: bool) -> Self {
		self.chrome.disabled = disabled;
		self
	}
}

/// Renders a labelled select.
pub fn select_field(props: SelectFieldProps) -> Page {
	let SelectFieldProps {
		chrome,
		value,
		options,
	} = props;
	let classes = control_classes(true, false);
	let id = chrome.id.clone();
	let label = chrome.label.clone();
	let disabled = chrome.disabled;
	let invalid = chrome.invalid();
	let described_by = chrome.described_by();
	let hint = hint_note(&chrome);
	let error = error_note(&chrome);
	page!({
		div {
			class: FIELD_STYLES.field(),
			label {
				class: FIELD_STYLES.label(),
				for: id.clone(),
				{ label }
			}
			select {
				id: id,
				class: classes,
				bind: value,
				disabled: disabled,
				aria_invalid: invalid,
				aria_describedby: described_by,
				for option in options {
					option {
						value: option.value.clone(),
						{ option.label.clone() }
					}
				}
			}
			{ hint }
			{ error }
		}
	})
}
