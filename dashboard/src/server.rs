//! Server entrypoint helpers shared between the production binary
//! (`src/main.rs`) and integration tests under `tests/`.
//!
//! The functions here boot the HTTP server the same way the production
//! container does: register the URL router from the `#[routes]`
//! inventory, then drive `RunServerCommand::execute`. Sharing this code
//! lets tests assert that the binary's startup path actually wires up a
//! router instead of duplicating reconnaissance into a parallel
//! implementation that drifts from production.

use std::collections::HashMap;
use std::error::Error;

use reinhardt::commands::{BaseCommand, CommandContext, RunServerCommand, auto_register_router};

/// Walk the `#[routes]` inventory and register the (single) discovered
/// router into the global slot that `RunServerCommand::execute` reads.
///
/// Thin wrapper around upstream [`auto_register_router`] preserved so
/// `tests/server_startup.rs` can assert that the binary's startup path
/// performs the inventory walk before delegating to `RunServerCommand`.
pub async fn register_router_from_inventory() -> Result<(), Box<dyn Error>> {
	auto_register_router().await
}

/// Build the `CommandContext` that drives `RunServerCommand::execute`.
///
/// Always sets the `noreload` option because the production container
/// image does not ship the `src/` tree that the upstream file watcher
/// expects. Without `noreload`, `RunServerCommand` defaults to autoreload
/// and tries to `notify::Watcher::watch("src/")`, which returns
/// `inotify_init1: No such file or directory` and aborts startup with
/// `File watcher error: No path was found`.
///
/// Developers who want hot-reload during local iteration should run
/// `cargo run --bin manage runserver` instead of the production binary —
/// the dashboard server entrypoint is for containerized deployment.
///
/// See kent8192/reinhardt-cloud#486 (issue 1).
pub(crate) fn build_context(bind_addr: &str) -> CommandContext {
	let mut options: HashMap<String, Vec<String>> = HashMap::new();
	// Empty value list — `RunServerCommand::execute` only checks key presence
	// via `ctx.has_option("noreload")`.
	options.insert("noreload".to_string(), Vec::new());
	CommandContext::new(vec![bind_addr.to_string()]).with_options(options)
}

/// Boot the dashboard HTTP server on `bind_addr`.
///
/// This is what `src/main.rs` calls in the container ENTRYPOINT and
/// what `tests/server_startup.rs` calls to assert that the entrypoint
/// performs router registration before delegating to
/// `RunServerCommand`.
pub async fn run(bind_addr: &str) -> Result<(), Box<dyn Error>> {
	register_router_from_inventory().await?;

	let ctx = build_context(bind_addr);
	let cmd = RunServerCommand;
	cmd.execute(&ctx)
		.await
		.map_err(|e| Box::<dyn Error>::from(e.to_string()))
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	// Refs #486 (issue 1): the production server entrypoint must disable
	// autoreload; otherwise the file watcher tries to inotify-watch
	// `src/`, which does not exist in container images.
	#[rstest]
	fn build_context_disables_autoreload() {
		// Arrange & Act
		let ctx = build_context("0.0.0.0:8000");

		// Assert
		assert!(
			ctx.has_option("noreload"),
			"production server entrypoint must disable autoreload; \
			 see kent8192/reinhardt-cloud#486"
		);
	}

	// Sanity: bind address must still be forwarded as the first positional
	// argument so `RunServerCommand::execute` picks it up unchanged.
	#[rstest]
	fn build_context_forwards_bind_address_as_first_arg() {
		// Arrange & Act
		let ctx = build_context("127.0.0.1:9000");

		// Assert
		assert_eq!(
			ctx.arg(0).map(String::as_str),
			Some("127.0.0.1:9000"),
			"bind address must be passed through unchanged so RunServerCommand binds correctly"
		);
	}
}
