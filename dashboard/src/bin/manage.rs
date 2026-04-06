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
use reinhardt_cloud_grpc::config::GrpcServerConfig;
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
	let is_runserver = std::env::args().nth(1).as_deref() == Some("runserver");

	// Fail fast if JWT secret is missing (better than per-request errors).
	// Only validate when running the server, not for management commands
	// like migrate or collectstatic.
	if is_runserver {
		reinhardt_cloud_dashboard::config::middleware::jwt_auth::JwtAuthMiddleware::validate_config(
		);
	}

	register_superuser_creator(superuser_creator_for::<User>());

	// When running the server, also start the gRPC server concurrently.
	if is_runserver {
		let grpc_config = GrpcServerConfig::default();
		let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

		let grpc_handle = tokio::spawn(async move {
			if let Err(e) =
				reinhardt_cloud_dashboard::config::grpc::start_grpc_server(grpc_config, async {
					drop(shutdown_rx.await)
				})
				.await
			{
				eprintln!("gRPC server error: {e}");
			}
		});

		let registry = CommandRegistry::new();
		let result = execute_from_command_line_with_registry(registry).await;

		// Signal gRPC server to shut down
		let _ = shutdown_tx.send(());
		let _ = grpc_handle.await;

		if let Err(e) = result {
			eprintln!("Error: {}", e);
			process::exit(1);
		}
	} else {
		let registry = CommandRegistry::new();
		if let Err(e) = execute_from_command_line_with_registry(registry).await {
			eprintln!("Error: {}", e);
			process::exit(1);
		}
	}
}
