//! Reinhardt Project Management CLI for cloud_control_plane
//!
//! This is the project-specific management command interface (equivalent to Django's manage.py).
//!
//! This binary is intentionally native-only. The whole module body is gated
//! behind `not(target_arch = "wasm32")` so that
//! `cargo check --target wasm32-unknown-unknown` on the workspace does not
//! try to compile a tokio-based CLI for the browser target. The wasm side
//! still requires a `main` symbol for `bin` crate-types, so we keep an
//! empty stub.
//!
//! ## Router Registration
//!
//! URL patterns are automatically registered by the framework.
//! No manual registration is required - see `src/config/urls.rs` for the
//! `#[routes]` attribute macro that enables this.

#[cfg(not(target_arch = "wasm32"))]
mod native {
	// Force-link the parent library so its `#[routes]` / `#[model]`
	// `inventory::submit!` registrations survive dead-code elimination.
	// Referencing `get_settings` alone does not guarantee the whole crate
	// (and thus every inventory entry) is linked.
	use cloud_control_plane as _;
	use cloud_control_plane::config::settings::{
		ProjectSettings, get_scoped_settings, get_settings,
	};
	#[cfg(feature = "commands-shell")]
	use cloud_control_plane::config::shell::get_shell_config;
	#[cfg(not(feature = "commands-shell"))]
	use reinhardt::commands::execute_from_command_line_with_capabilities;
	#[cfg(feature = "commands-shell")]
	use reinhardt::commands::execute_from_command_line_with_capabilities_and_shell;
	use reinhardt::commands::{
		CapabilityProvider, CargoCheckContext, CommandRegistry, command_error_exit_code,
	};
	use reinhardt::conf::settings::PendingSettings;
	use reinhardt::conf::settings::builder::BuildError;
	use reinhardt::conf::settings::scoped::ScopedSettings;
	use std::path::PathBuf;
	use std::process;

	struct ProjectProvider;

	impl CapabilityProvider for ProjectProvider {
		type Settings = ProjectSettings;

		fn scoped_settings(&self) -> Result<ScopedSettings, BuildError> {
			get_scoped_settings()
		}

		fn full_settings(&self) -> Result<PendingSettings<ProjectSettings>, BuildError> {
			get_settings()
		}
	}

	#[tokio::main]
	pub(super) async fn main() {
		// The command is selected before either settings provider runs.
		// Static commands resolve selected asset inputs; runtime commands
		// retain the full composed-settings validation path.
		let cargo_context = CargoCheckContext::from_launcher(
			PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
			Some(env!("CARGO_PKG_NAME").to_owned()),
			Some("manage".to_owned()),
		);
		#[cfg(feature = "commands-shell")]
		let result = execute_from_command_line_with_capabilities_and_shell(
			CommandRegistry::new(),
			ProjectProvider,
			Some(cargo_context),
			get_shell_config(),
		)
		.await;
		#[cfg(not(feature = "commands-shell"))]
		let result = execute_from_command_line_with_capabilities(
			CommandRegistry::new(),
			ProjectProvider,
			Some(cargo_context),
		)
		.await;

		if let Err(e) = result {
			let exit_code = command_error_exit_code(e.as_ref());
			eprintln!("Error: {}", e);
			process::exit(exit_code);
		}
	}
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
	// SAFETY: first statement of `main`, before the shell runtime hook and the
	// Tokio runtime exist, so no other thread can access the process environment.
	unsafe {
		std::env::set_var(
			"REINHARDT_SETTINGS_MODULE",
			"cloud_control_plane.config.settings",
		);
	}

	reinhardt::commands::shell_runtime_hook();
	native::main();
}

#[cfg(target_arch = "wasm32")]
fn main() {}
