//! English message catalog.
//!
//! The first element of each pair is the message ID used with `t!`; the second
//! is its English rendering. Keep the list sorted by feature so a missing
//! entry is easy to spot; the `every_t_message_has_an_english_entry` test in
//! `tests/i18n.rs` fails when a `t!` literal in `src/` has no entry here.

/// Message ID and English rendering pairs.
pub const MESSAGES: &[(&str, &str)] = &[
	// Product
	("Reinhardt Cloud", "Reinhardt Cloud"),
	// Signed-out layout
	("Skip to main content", "Skip to main content"),
	// Theme toggle
	("Switch to dark theme", "Switch to dark theme"),
	("Switch to light theme", "Switch to light theme"),
	// Code block
	("Copy", "Copy"),
	("Copied", "Copied"),
];
