//! Modal dialog on the native `<dialog>` element, mounted through a portal.
//!
//! `reinhardt-pages` checked: `Portal` / `mount_portal` mount a view outside the
//! caller's tree and return a `PortalHandle` that removes it when dropped. There
//! is no dialog primitive, so this module supplies the `<dialog>` markup
//! ([`dialog_view`]) and the open/close lifecycle ([`open_dialog`]) on top of
//! the portal. The browser traps focus inside a modal dialog and closes it on
//! `Escape`.
//!
//! Portals render only a placeholder on the server target and mount nothing, so
//! [`open_dialog`] returns an inactive handle there; the markup is covered by
//! native tests of [`dialog_view`] and the lifecycle by browser tests.

use reinhardt::pages::component::Page;
use reinhardt::pages::{
	ClassList, PortalError, PortalHandle, PortalTarget, TranslatedText, mount_portal, page,
	style_def,
};

use crate::components::browser;

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

/// Properties of a dialog.
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

/// Renders the closed `<dialog>` element for `props`.
pub fn dialog_view(props: DialogProps) -> Page {
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

/// A dialog that is open until this handle is dropped.
///
/// Dropping the handle unmounts the portal and removes the dialog from the
/// document, which also releases the focus trap.
pub struct OpenDialog {
	portal: PortalHandle,
}

impl OpenDialog {
	/// Reports whether the dialog is mounted in a document.
	///
	/// Always `false` on the server target, where portals mount nothing.
	pub fn is_open(&self) -> bool {
		self.portal.is_active()
	}
}

/// Mounts the dialog under `<body>` and opens it as a modal.
///
/// Call it inside a reactive scope. Keep the returned handle for as long as the
/// dialog should exist; drop it to close the dialog.
///
/// # Errors
///
/// Returns the portal error when the document has no `<body>` or the view
/// cannot be mounted.
pub fn open_dialog(props: DialogProps) -> Result<OpenDialog, PortalError> {
	let id = props.id.clone();
	let portal = mount_portal(PortalTarget::body(), dialog_view(props))?;
	browser::show_modal(&id);
	Ok(OpenDialog { portal })
}
