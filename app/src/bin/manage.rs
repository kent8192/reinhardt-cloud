//! Reinhardt Project Management CLI for nuages
//!
//! This is the project-specific management command interface (equivalent to Django's manage.py).
//!
//! ## Router Registration
//!
//! URL patterns are automatically registered by the framework.
//! No manual registration is required - see `src/config/urls.rs` for the
//! `#[routes]` attribute macro that enables this.
//!
//! ## Custom Commands
//!
//! Project-specific commands (e.g., `SyncClustersCommand`) are registered
//! in the `CommandRegistry` before the CLI is executed.

use nuages as _;
use reinhardt::commands::{BaseCommand, CommandContext, CommandRegistry, CommandResult, execute_from_command_line};
use reinhardt::core::async_trait;
use std::process;

/// Custom management command to synchronize cluster status from the Kubernetes API.
struct SyncClustersCommand;

#[async_trait]
impl BaseCommand for SyncClustersCommand {
	fn name(&self) -> &str {
		"sync_clusters"
	}

	fn description(&self) -> &str {
		"Sync cluster status from Kubernetes API"
	}

	async fn execute(&self, ctx: &CommandContext) -> CommandResult<()> {
		ctx.info("Syncing cluster status from Kubernetes...");
		Ok(())
	}
}

#[tokio::main]
async fn main() {
	// Set settings module environment variable
	// SAFETY: This is safe because we're setting it before any other code runs
	unsafe {
		std::env::set_var("REINHARDT_SETTINGS_MODULE", "nuages.config.settings");
	}

	// Register project-specific custom commands
	let mut registry = CommandRegistry::new();
	registry.register(Box::new(SyncClustersCommand));

	// Router registration happens automatically inside execute_from_command_line()
	// via the #[routes] attribute macro in src/config/urls.rs
	if let Err(e) = execute_from_command_line().await {
		eprintln!("Error: {}", e);
		process::exit(1);
	}
}
