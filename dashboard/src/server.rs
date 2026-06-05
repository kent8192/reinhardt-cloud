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
use reinhardt::db::orm;

/// Delegate to the upstream [`auto_register_router`] helper, which walks
/// the `#[routes]` inventory and registers the discovered router into the
/// global slot that `RunServerCommand::execute` reads.
///
/// Kept as a named entry point (rather than calling `auto_register_router`
/// directly from `run`) so `tests/server_startup.rs` can assert that the
/// binary's startup path performs router registration before delegating to
/// `RunServerCommand`. The actual registration mechanism lives in
/// `reinhardt-commands` upstream.
pub async fn register_router_from_inventory() -> Result<(), Box<dyn Error>> {
	auto_register_router().await
}

/// Static-files directory baked into the runtime container image by the
/// Dockerfile generator's pages branch. Must stay in sync with
/// `crates/reinhardt-cloud-cli/src/dockerfile_generator/stages.rs`'s
/// `build_runtime_stage` (`/app/static/wasm/` COPY destination).
///
/// See kent8192/reinhardt-cloud#511.
pub(crate) const PAGES_STATIC_DIR: &str = "/app/static/wasm";

/// Build the `CommandContext` that drives `RunServerCommand::execute`.
///
/// Always sets the `noreload` option because the production container
/// image does not ship the `src/` tree that the upstream file watcher
/// expects. Without `noreload`, `RunServerCommand` defaults to autoreload
/// and tries to `notify::Watcher::watch("src/")`, which returns
/// `inotify_init1: No such file or directory` and aborts startup with
/// `File watcher error: No path was found`.
///
/// Always sets `with-pages` and `static-dir` so the WASM frontend that
/// the Dockerfile generator ships into the runtime image is actually
/// reachable. Without these, `RunServerCommand` runs without WASM
/// serving, the `/app/static/wasm/` payload is dead weight, and the
/// SPA fallback never resolves `index.html`. See
/// kent8192/reinhardt-cloud#511.
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
	// Flag option: `RunServerCommand::execute` reads it via `ctx.has_option("with-pages")`.
	options.insert("with-pages".to_string(), Vec::new());
	// Value option: `RunServerCommand::execute` reads it via `ctx.option("static-dir")`,
	// which returns the first value of the Vec.
	options.insert("static-dir".to_string(), vec![PAGES_STATIC_DIR.to_string()]);
	CommandContext::new(vec![bind_addr.to_string()]).with_options(options)
}

/// Initialize the global ORM pool before handing control to `RunServerCommand`.
///
/// The normal `manage runserver` path initializes the ORM before command
/// dispatch. The container entrypoint calls `RunServerCommand` directly so it
/// can avoid the management CLI parser, which means it must perform the same
/// database setup explicitly before runserver registers `DatabaseConnection`
/// in DI and before health probes exercise the ORM.
async fn initialize_orm_database() -> Result<(), Box<dyn Error>> {
	let env_database_url = std::env::var("DATABASE_URL").ok();
	let settings = crate::config::settings::get_settings();
	let url = match env_database_url.as_deref() {
		Some(url) if !url.is_empty() => url.to_string(),
		_ => settings
			.core
			.databases
			.get("default")
			.ok_or("settings must define core.databases.default")?
			.to_url(),
	};

	if env_database_url.as_deref() != Some(url.as_str()) {
		// SAFETY: `run` calls this before spawning runserver tasks, so no other
		// application thread is concurrently reading or mutating environment.
		unsafe {
			std::env::set_var("DATABASE_URL", &url);
		}
	}

	orm::init_database(&url)
		.await
		.map_err(|e| format!("failed to initialize ORM database: {e}"))?;
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
	initialize_orm_database().await?;

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

	// Refs #511: the production server entrypoint must enable WASM frontend
	// serving so the artifacts the Dockerfile generator copies into
	// `/app/static/wasm/` are actually reachable.
	#[rstest]
	fn build_context_enables_pages_serving() {
		// Arrange & Act
		let ctx = build_context("0.0.0.0:8000");

		// Assert
		assert!(
			ctx.has_option("with-pages"),
			"production server entrypoint must enable WASM frontend serving; \
			 see kent8192/reinhardt-cloud#511"
		);
	}

	// Refs #511: the static-dir literal must match the Dockerfile generator's
	// COPY destination in `crates/reinhardt-cloud-cli/src/dockerfile_generator/
	// stages.rs::build_runtime_stage`. If either side drifts, this assertion
	// fails before users hit a broken container image.
	#[rstest]
	fn build_context_sets_static_dir_to_runtime_wasm_path() {
		// Arrange & Act
		let ctx = build_context("0.0.0.0:8000");

		// Assert
		assert_eq!(
			ctx.option("static-dir").map(String::as_str),
			Some(PAGES_STATIC_DIR),
			"static-dir must point at the Dockerfile generator's COPY destination; \
			 see kent8192/reinhardt-cloud#511"
		);
	}
}
