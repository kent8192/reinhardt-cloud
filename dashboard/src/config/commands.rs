//! The `manage` command registry.
//!
//! One line per application that contributes commands, like the router
//! composition in `config/urls.rs`. `src/bin/manage.rs` calls [`registry`] and
//! hands the result to the command driver.

use reinhardt::commands::CommandRegistry;

/// Build the registry of project-specific `manage` commands.
#[must_use]
pub fn registry() -> CommandRegistry {
	let mut registry = CommandRegistry::new();
	crate::apps::accounts::server::commands::register(&mut registry);
	registry
}
