//! Server entrypoint for containerized dashboard deployment.
//!
//! Built into the `reinhardt-cloud-dashboard` binary that the
//! auto-generated `Dockerfile` invokes via its `ENTRYPOINT`. The binary
//! exists separately from `src/bin/manage.rs` so that operational
//! tooling (`makemigrations`, `migrate`, `shell`, ...) and the
//! production HTTP server are decoupled — running the container does
//! not start the management CLI parser, and adding management
//! subcommands cannot accidentally change the deployment ENTRYPOINT
//! shape.
//!
//! The binary listens on `0.0.0.0:8000`; this address matches
//! `manifests/dashboard-app.yaml::services.target_port` and the
//! `EXPOSE 8000` directive that the auto-generated `Dockerfile`
//! produces from the same `reinhardt-cloud.toml`.

#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	// SAFETY: invoked at program start before any spawned tasks read
	// environment variables. Mirrors `src/bin/manage.rs`.
	unsafe {
		std::env::set_var(
			"REINHARDT_SETTINGS_MODULE",
			"reinhardt_cloud_dashboard.config.settings",
		);
	}

	// Bind to all interfaces so the Kubernetes Service can route to us.
	// `server::run` registers the router from the inventory before
	// invoking `RunServerCommand::execute` — without that step the
	// server binary fails immediately with "No router registered"
	// (see kent8192/reinhardt-cloud#478).
	reinhardt_cloud_dashboard::server::run("0.0.0.0:8000")
		.await
		.map_err(|e| {
			eprintln!("dashboard server failed: {e}");
			e
		})
}

/// On `wasm32` the dashboard ships only its `[lib]` (cdylib) artifact.
/// The binary is server-only, but Cargo still tries to compile every
/// auto-discovered `src/main.rs` for the requested target — so provide
/// a no-op `main` here that satisfies the linker without pulling in any
/// of the server-only dependencies.
#[cfg(target_arch = "wasm32")]
fn main() {}
