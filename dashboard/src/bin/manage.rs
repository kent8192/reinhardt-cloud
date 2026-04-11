//! Reinhardt Project Management CLI for Reinhardt Cloud
//!
//! This is the project-specific management command interface (equivalent to Django's manage.py).
//!
//! ## Router Registration
//!
//! URL patterns are automatically registered by the framework.
//! No manual registration is required - see `src/config/urls.rs` for the
//! `#[routes]` attribute macro that enables this.

use reinhardt::commands::execute_from_command_line;
use reinhardt_cloud_dashboard as _;
use std::process;

#[tokio::main]
async fn main() {
	// SAFETY: Called at program start before any spawned tasks.
	// env::set_var is safe in single-threaded context.
	unsafe {
		std::env::set_var(
			"REINHARDT_SETTINGS_MODULE",
			"reinhardt_cloud_dashboard.config.settings",
		);
	}

	if let Err(e) = execute_from_command_line().await {
		eprintln!("Error: {e}");
		process::exit(1);
	}
}
