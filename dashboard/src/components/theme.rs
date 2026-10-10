//! Light and dark theme selection.
//!
//! `reinhardt-pages` checked: `ui::ActionButton` dispatches an `Action`, and the
//! toggle is a synchronous document and storage update, so a styled `button`
//! with a click handler is used. No primitive exists for theme state.
//!
//! The theme follows the system setting until a User picks one. A picked theme
//! is written to the `data-theme` attribute of the document root, which the
//! tokens in `static/css/tokens.css` switch on, and remembered in local
//! storage. `static/js/theme-init.js` re-applies the stored choice before the
//! first paint.

use reinhardt::pages::component::Page;
use reinhardt::pages::event::ClickEvent;
use reinhardt::pages::reactive::Signal;
use reinhardt::pages::{Callback, page, t};

use crate::components::browser;
use crate::components::button::{ButtonSize, ButtonVariant, button_classes};

/// Local storage key of the remembered theme; keep in sync with
/// `static/js/theme-init.js`.
pub const STORAGE_KEY: &str = "rc-theme";

/// A color theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
	/// Light surfaces.
	Light,
	/// Dark surfaces.
	Dark,
}

impl Theme {
	/// Returns the `data-theme` attribute value.
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Light => "light",
			Self::Dark => "dark",
		}
	}

	/// Parses a `data-theme` or stored value; anything else is `None`.
	pub fn parse(value: &str) -> Option<Self> {
		match value {
			"light" => Some(Self::Light),
			"dark" => Some(Self::Dark),
			_ => None,
		}
	}

	/// Returns the other theme.
	pub fn toggled(self) -> Self {
		match self {
			Self::Light => Self::Dark,
			Self::Dark => Self::Light,
		}
	}
}

/// Returns the theme in effect: the document's explicit choice, else the
/// system setting.
pub fn current_theme() -> Theme {
	browser::document_theme()
		.as_deref()
		.and_then(Theme::parse)
		.unwrap_or(if browser::system_prefers_dark() {
			Theme::Dark
		} else {
			Theme::Light
		})
}

/// Applies `theme` to the document and remembers it.
pub fn apply_theme(theme: Theme) {
	browser::set_document_theme(theme.as_str());
	browser::write_stored(STORAGE_KEY, theme.as_str());
}

/// Renders the button that switches to the other theme.
pub fn theme_toggle() -> Page {
	let theme = Signal::new(current_theme());
	// `Signal` is `Copy`, so the handler owns its own handle.
	let toggle = Callback::new(move |_event: ClickEvent| {
		let next = theme.get_untracked().toggled();
		apply_theme(next);
		theme.set(next);
	});
	let classes = button_classes(ButtonVariant::Secondary, ButtonSize::Small);
	page!({
		button {
			class: classes,
			type: "button",
			@click: toggle,
			if theme.get() == Theme::Dark { { t!("Switch to light theme") } } else { { t!("Switch to dark theme") } }
		}
	})
}
