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

use std::cell::RefCell;
use std::rc::Rc;

use reinhardt::pages::component::Page;
use reinhardt::pages::{
	ClassList, PortalError, PortalHandle, PortalTarget, TranslatedText, mount_portal, page,
	style_def,
};

use crate::components::browser::{self, EventListener};

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
		border_width: Length;
		measure_dialog: Length;
		space_6: Length;
		tracking_display: Length;
	}
	.dialog {
		width: min(globals.measure_dialog, 100vw - globals.space_6);
		padding: globals.space_5;
		background: globals.surface_raised;
		border: (globals.border_width, solid, globals.border_default);
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
		letter-spacing: globals.tracking_display;
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

/// A dialog that is open until it closes or this handle is dropped.
///
/// Dropping the handle unmounts the portal and removes the dialog from the
/// document, which also releases the focus trap. When the browser closes the
/// dialog itself (`Escape`, `<form method="dialog">`), the portal is torn down
/// and the `on_close` callback runs.
pub struct OpenDialog {
	// Declared before `portal` so the listener is removed before the dialog.
	_close_listener: Option<EventListener>,
	portal: Rc<RefCell<Option<PortalHandle>>>,
}

impl OpenDialog {
	/// Reports whether the dialog is still mounted in a document.
	///
	/// Always `false` on the server target, where portals mount nothing.
	pub fn is_open(&self) -> bool {
		self.portal
			.borrow()
			.as_ref()
			.is_some_and(PortalHandle::is_active)
	}
}

/// Mounts the dialog under `<body>` and opens it as a modal.
///
/// Call it inside a reactive scope. Keep the returned handle for as long as the
/// dialog should exist; drop it to close the dialog. `on_close` runs after the
/// browser closes the dialog on its own (not when the handle is dropped).
///
/// # Errors
///
/// Returns the portal error when the document has no `<body>` or the view
/// cannot be mounted.
pub fn open_dialog(
	props: DialogProps,
	on_close: impl Fn() + 'static,
) -> Result<OpenDialog, PortalError> {
	let id = props.id.clone();
	let portal = Rc::new(RefCell::new(Some(mount_portal(
		PortalTarget::body(),
		dialog_view(props),
	)?)));
	browser::show_modal(&id);
	let close_listener = browser::on_dialog_close(&id, {
		let portal = Rc::clone(&portal);
		move || {
			// Dropping the handle removes the portal host and the dialog.
			portal.borrow_mut().take();
			on_close();
		}
	});
	Ok(OpenDialog {
		_close_listener: close_listener,
		portal,
	})
}
