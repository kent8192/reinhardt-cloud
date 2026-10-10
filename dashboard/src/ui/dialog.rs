//! Modal dialog on the native `<dialog>` element.
//!
//! The browser traps focus inside a modal dialog and closes it on `Escape`.
//! Open it with [`show_modal`] and close it with [`close`], passing the same ID
//! given to [`DialogProps::new`].

use reinhardt::pages::component::Page;
use reinhardt::pages::{ClassList, TranslatedText, page, style_def};

use crate::ui::browser;

#[style_def]
pub static DIALOG_STYLES: DialogStyles = style! {
	globals {
		space_2: Length;
		space_3: Length;
		space_5: Length;
		radius_xl: Length;
		surface_raised: Color;
		border_default: Color;
		ink_default: Color;
		ink_strong: Color;
		ink_muted: Color;
		scrim: Color;
		text_xl: Length;
		leading_prose: Number;
		weight_regular: Number;
	}
	.dialog {
		width: min(32rem, 100vw - 2rem);
		padding: globals.space_5;
		background: globals.surface_raised;
		border: (1px, solid, globals.border_default);
		border-radius: globals.radius_xl;
		color: globals.ink_default;
		&::backdrop {
			background: globals.scrim;
		}
	}
	.title {
		color: globals.ink_strong;
		font-size: globals.text_xl;
		font-weight: globals.weight_regular;
		letter-spacing: -0.2px;
	}
	.body {
		margin: (globals.space_3, 0, globals.space_5);
		line-height: globals.leading_prose;
		color: globals.ink_muted;
	}
	.actions {
		display: flex;
		justify-content: flex-end;
		gap: globals.space_2;
	}
};

/// Properties of a [`dialog`].
#[derive(Clone)]
pub struct DialogProps {
	id: String,
	title: TranslatedText,
	body: Page,
	actions: Page,
}

impl DialogProps {
	/// Creates properties for a dialog.
	///
	/// `id` must be unique in the document. `actions` holds the buttons shown
	/// at the end of the dialog.
	pub fn new(id: impl Into<String>, title: TranslatedText, body: Page, actions: Page) -> Self {
		Self {
			id: id.into(),
			title,
			body,
			actions,
		}
	}
}

/// Returns the class list of a dialog element.
pub fn dialog_classes() -> ClassList {
	DIALOG_STYLES.dialog() + "rc-elevation-2" + "rc-dialog-enter"
}

/// Renders a closed dialog; open it with [`show_modal`].
pub fn dialog(props: DialogProps) -> Page {
	let DialogProps {
		id,
		title,
		body,
		actions,
	} = props;
	let title_id = format!("{id}-title");
	let classes = dialog_classes();
	page!({
		dialog {
			id: id,
			class: classes,
			aria_labelledby: title_id.clone(),
			h2 {
				id: title_id,
				class: DIALOG_STYLES.title(),
				{ title }
			}
			div {
				class: DIALOG_STYLES.body(),
				{ body }
			}
			div {
				class: DIALOG_STYLES.actions(),
				{ actions }
			}
		}
	})
}

/// Opens the dialog with the given ID as a modal.
pub fn show_modal(id: &str) {
	browser::show_modal(id);
}

/// Closes the dialog with the given ID.
pub fn close(id: &str) {
	browser::close_dialog(id);
}
