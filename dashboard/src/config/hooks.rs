//! Runserver lifecycle hooks for Reinhardt Cloud Dashboard.
//!
//! Hooks are auto-discovered via `inventory` and invoked by the framework's
//! `RunServerCommand` at two lifecycle points:
//!
//! 1. **Validation** — [`RedisValidationHook`] checks required config before DI setup.
//! 2. **Startup** — [`WebSocketRunserverHook`] registers Dashboard WebSocket routes.
//! 3. **Startup** — [`GrpcRunserverHook`] spawns the gRPC server alongside HTTP.

use std::error::Error;

use async_trait::async_trait;
use reinhardt::commands::{RunserverContext, RunserverHook, RunserverHookRegistration};
use reinhardt_cloud_grpc::config::GrpcServerConfig;

use super::grpc::start_grpc_server;
use super::settings::get_redis_url;
use super::urls::init_websocket_routes;

/// Validates that a Redis URL is configured before the server starts.
///
/// Fails fast during the validation phase so that a missing Redis URL
/// surfaces immediately rather than causing per-request errors at runtime.
pub struct RedisValidationHook;

inventory::submit! {
	RunserverHookRegistration::__macro_new(
		|| Box::new(RedisValidationHook),
		"RedisValidationHook",
	)
}

#[async_trait]
impl RunserverHook for RedisValidationHook {
	async fn validate(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
		get_redis_url().ok_or::<Box<dyn Error + Send + Sync>>(
			"Redis URL must be configured: set REINHARDT_CLOUD_REDIS_URL env var \
			 or redis_url in settings TOML"
				.into(),
		)?;
		Ok(())
	}
}

/// Registers Dashboard WebSocket routes for the management `runserver` path.
///
/// The container entrypoint calls `server::run`, which registers WebSocket
/// routes directly before delegating to `RunServerCommand`. Local development
/// starts through `manage runserver`, so the same registration must also run
/// as a framework startup hook; otherwise `/ws/notifications` falls through to
/// the SPA fallback and returns HTTP 200 instead of upgrading. Refs #666.
pub struct WebSocketRunserverHook;

inventory::submit! {
	RunserverHookRegistration::__macro_new(
		|| Box::new(WebSocketRunserverHook),
		"WebSocketRunserverHook",
	)
}

#[async_trait]
impl RunserverHook for WebSocketRunserverHook {
	async fn on_server_start(
		&self,
		ctx: &RunserverContext,
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		let _ = ctx;
		init_websocket_routes().await;
		Ok(())
	}
}

/// Starts the gRPC server as a concurrent service alongside the HTTP server.
///
/// Subscribes to the framework's shutdown coordinator so the gRPC server
/// shuts down gracefully when the main server exits.
pub struct GrpcRunserverHook;

inventory::submit! {
	RunserverHookRegistration::__macro_new(
		|| Box::new(GrpcRunserverHook),
		"GrpcRunserverHook",
	)
}

#[async_trait]
impl RunserverHook for GrpcRunserverHook {
	async fn on_server_start(
		&self,
		ctx: &RunserverContext,
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		let mut shutdown_rx = ctx.shutdown_coordinator.subscribe();
		let grpc_config = GrpcServerConfig::default();
		// `tokio::spawn` does not propagate task-local storage, so the
		// gRPC startup task cannot rely on `get_di_context`. Capture the
		// root DI context here and pass it explicitly into the spawned
		// task instead.
		let di_context = ctx.di_context.clone();

		tokio::spawn(async move {
			if let Err(e) = start_grpc_server(grpc_config, di_context, async move {
				let _ = shutdown_rx.recv().await;
			})
			.await
			{
				eprintln!("gRPC server error: {e}");
			}
		});

		Ok(())
	}
}
