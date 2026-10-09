//! Production HTTP server bootstrap.
//!
//! The `reinhardt-cloud-dashboard` binary is a thin launcher around
//! [`run`]. Everything that registers with the DI container, the router
//! inventory, or the settings system lives in this library crate so that the
//! binary and `manage` observe the same registrations.

use std::collections::HashMap;
use std::error::Error;

use reinhardt::commands::{BaseCommand, CommandContext, RunServerCommand};

/// Directory holding the WASM bundle and `index.html` inside the runtime image.
const PAGES_STATIC_DIR: &str = "/app/static/wasm";

/// Build the `runserver` context used by the container entry point.
///
/// Autoreload and the startup WASM build are disabled because the runtime image ships
/// neither a `src/` tree to watch nor a Rust toolchain; the prebuilt Pages
/// bundle is served from [`PAGES_STATIC_DIR`].
fn build_context(bind_addr: &str) -> CommandContext {
	let mut options: HashMap<String, Vec<String>> = HashMap::new();
	options.insert("noreload".to_owned(), Vec::new());
	options.insert("with-pages".to_owned(), Vec::new());
	options.insert("no-wasm".to_owned(), Vec::new());
	options.insert("static-dir".to_owned(), vec![PAGES_STATIC_DIR.to_owned()]);
	CommandContext::new(vec![bind_addr.to_owned()]).with_options(options)
}

/// Run the HTTP server on `bind_addr` until shutdown.
///
/// # Errors
///
/// Returns an error when route registration or the server itself fails.
pub async fn run(bind_addr: &str) -> Result<(), Box<dyn Error>> {
	RunServerCommand.execute(&build_context(bind_addr)).await?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use rstest::rstest;

	use super::{PAGES_STATIC_DIR, build_context};

	#[rstest]
	fn build_context_serves_prebuilt_pages_bundle() {
		// Arrange
		let bind_addr = "0.0.0.0:8000";

		// Act
		let ctx = build_context(bind_addr);

		// Assert
		assert_eq!(ctx.arg(0).map(String::as_str), Some(bind_addr));
		assert!(ctx.has_option("noreload"));
		assert!(ctx.has_option("with-pages"));
		assert!(ctx.has_option("no-wasm"));
		assert_eq!(
			ctx.option("static-dir").map(String::as_str),
			Some(PAGES_STATIC_DIR)
		);
	}
}
