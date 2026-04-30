//! Server entrypoint helpers shared between the production binary
//! (`src/main.rs`) and integration tests under `tests/`.
//!
//! The functions here boot the HTTP server the same way the production
//! container does: register the URL router from the `#[routes]`
//! inventory, then drive `RunServerCommand::execute`. Sharing this code
//! lets tests assert that the binary's startup path actually wires up a
//! router instead of duplicating reconnaissance into a parallel
//! implementation that drifts from production.

use std::error::Error;

use reinhardt::commands::{BaseCommand, CommandContext, RunServerCommand};
use reinhardt::urls::routers::{UrlPatternsRegistration, register_router_arc};

/// Walk the `#[routes]` inventory and register the (single) discovered
/// router into the global slot that `RunServerCommand::execute` reads.
///
/// This function is the [WP-3] workaround replicating
/// `reinhardt-commands`'s private `auto_register_router`. Keep the
/// error messages aligned with the upstream copy so debugging guidance
/// stays consistent across both call paths (CLI vs. direct binary).
///
// Workaround for kent8192/reinhardt-web#4055
//   (tracked in reinhardt-cloud#479)
//
// `RunServerCommand::execute()` does not auto-register the URL router because
// the `auto_register_router` helper is private to reinhardt-commands. Until
// the upstream exposes a public auto-register or `start_server(addr)` helper,
// we replicate the inventory walk here so direct `RunServerCommand::execute()`
// callers also boot a usable router.
//
// Ideal implementation (without workaround):
//   reinhardt::urls::routers::auto_register_router().await?;
//   RunServerCommand.execute(&ctx).await?;
pub async fn register_router_from_inventory() -> Result<(), Box<dyn Error>> {
	// Collect every `#[routes]` registration the linker preserved.
	let registrations: Vec<&UrlPatternsRegistration> =
		inventory::iter::<UrlPatternsRegistration>().collect();

	match registrations.len() {
		0 => {
			return Err("No URL patterns registered.\n\
				 Add the `#[routes]` attribute to your routes function in src/config/urls.rs:\n\n\
				 #[routes]\n\
				 pub fn routes() -> UnifiedRouter {\n\
				     UnifiedRouter::new()\n\
				 }\n\n\
				 If your project uses a library/binary split (src/lib.rs + src/bin/manage.rs),\n\
				 the linker may silently discard route registrations from the library crate.\n\
				 Fix: add `use your_crate_name as _;` to src/bin/manage.rs to force-link\n\
				 the library and preserve its side-effectful route registrations."
				.to_string()
				.into());
		}
		1 => {
			// Expected case: exactly one registration. Continue below.
		}
		n => {
			return Err(format!(
				"Multiple #[routes] functions detected ({n} found).\n\
				 Only one function in the entire project should be annotated with #[routes].\n\n\
				 Please ensure that:\n\
				 1. Only one #[routes] attribute exists in your codebase\n\
				 2. Check src/config/urls.rs and any other files that might have #[routes]\n\
				 3. If you have multiple router configurations, combine them into a single function"
			)
			.into());
		}
	}

	let registration = registrations[0];
	let router = registration
		.server_router_async()
		.await
		.map_err(|e| format!("Failed to create router from #[routes] function: {e}"))?;
	register_router_arc(router);

	Ok(())
}

/// Boot the dashboard HTTP server on `bind_addr`.
///
/// This is what `src/main.rs` calls in the container ENTRYPOINT and
/// what `tests/server_startup.rs` calls to assert that the entrypoint
/// performs router registration before delegating to
/// `RunServerCommand`.
pub async fn run(bind_addr: &str) -> Result<(), Box<dyn Error>> {
	register_router_from_inventory().await?;

	let ctx = CommandContext::new(vec![bind_addr.to_string()]);
	let cmd = RunServerCommand;
	cmd.execute(&ctx)
		.await
		.map_err(|e| Box::<dyn Error>::from(e.to_string()))
}
