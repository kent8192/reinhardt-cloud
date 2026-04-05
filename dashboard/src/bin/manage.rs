//! Reinhardt Project Management CLI for Reinhardt Cloud
//!
//! This is the project-specific management command interface (equivalent to Django's manage.py).
//!
//! ## Router Registration
//!
//! URL patterns are automatically registered by the framework.
//! No manual registration is required - see `src/config/urls.rs` for the
//! `#[routes]` attribute macro that enables this.

use reinhardt::commands::CommandRegistry;
use reinhardt::commands::execute_from_command_line_with_registry;
use reinhardt::reinhardt_auth::{register_superuser_creator, superuser_creator_for};
use reinhardt_cloud_dashboard::apps::auth::models::user::User;
use std::process;

fn main() {
	// SAFETY: Called before tokio runtime initialization, so no other
	// threads exist. env::set_var is safe in single-threaded context.
	unsafe {
		std::env::set_var(
			"REINHARDT_SETTINGS_MODULE",
			"reinhardt_cloud_dashboard.config.settings",
		);
	}

	tokio::runtime::Builder::new_multi_thread()
		.enable_all()
		.build()
		.expect("Failed to build tokio runtime")
		.block_on(async_main());
}

async fn async_main() {
	// Fail fast if JWT secret is missing (better than per-request errors).
	// Only validate when running the server, not for management commands
	// like migrate or collectstatic.
	if std::env::args().nth(1).as_deref() == Some("runserver") {
		reinhardt_cloud_dashboard::config::middleware::jwt_auth::JwtAuthMiddleware::validate_config();
	}

	register_superuser_creator(superuser_creator_for::<User>());

	let registry = CommandRegistry::new();
	if let Err(e) = execute_from_command_line_with_registry(registry).await {
		eprintln!("Error: {}", e);
		process::exit(1);
	}
}
