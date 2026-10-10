//! Internationalization foundation for the Control Plane UI.
//!
//! Every user-visible string is rendered through `t!` (or `tr`, `tn`, `tp`,
//! `tnp`) so it follows the active locale of the [`I18nContext`] installed by
//! the client launcher. English ships first: the message ID is the English
//! source text, and [`en`] lists every message with its English rendering. Add
//! a locale by providing one more catalog next to `en` and registering it in
//! [`translation_context`].
//!
//! The catalog is compiled into the library because the browser bundle has no
//! file system to load `.po` files from.

pub mod en;

use reinhardt::pages::i18n::{I18nContext, MessageCatalog, TranslationContext};

/// Locale shipped first and used as the fallback for every other locale.
pub const DEFAULT_LOCALE: &str = "en-US";

/// Builds the English message catalog.
pub fn english_catalog() -> MessageCatalog {
	let mut catalog = MessageCatalog::new(DEFAULT_LOCALE);
	for (message, translation) in en::MESSAGES {
		catalog.add_translation(*message, *translation);
	}
	catalog
}

/// Builds the translation context holding every shipped catalog.
pub fn translation_context() -> TranslationContext {
	let mut translations = TranslationContext::new(DEFAULT_LOCALE, DEFAULT_LOCALE);
	translations
		.add_catalog(DEFAULT_LOCALE, english_catalog())
		.expect("the default locale tag is valid");
	translations
}

/// Builds the page i18n context installed by the client launcher.
pub fn i18n_context() -> I18nContext {
	I18nContext::new(translation_context())
}
