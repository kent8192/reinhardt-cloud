//! Server entry point for the containerized Control Plane.
//!
//! This binary only launches the HTTP server defined in the library crate.
//! It must not define DI-registered types: a type registered from a binary
//! crate is invisible to the `manage` binary and to tests. It listens on
//! `0.0.0.0:8000`, the port the image layout and Kubernetes Service expect.

#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	// SAFETY: called at program start before any task reads the environment.
	unsafe {
		std::env::set_var(
			"REINHARDT_SETTINGS_MODULE",
			"cloud_control_plane.config.settings",
		);
	}

	cloud_control_plane::server::run("0.0.0.0:8000").await
}

/// The server binary is native-only; Cargo still compiles auto-discovered
/// binaries for the WASM target, so provide an empty entry point there.
#[cfg(target_arch = "wasm32")]
fn main() {}
