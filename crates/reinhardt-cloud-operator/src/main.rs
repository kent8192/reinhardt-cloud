//! Reinhardt Cloud Kubernetes operator for managing `ReinhardtApp` resources.

mod error;
mod inference;
mod reconciler;
mod resources;

use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	fmt()
		.with_env_filter(
			EnvFilter::from_default_env().add_directive("reinhardt_cloud_operator=info".parse()?),
		)
		.init();

	tracing::info!("Starting reinhardt-cloud operator");

	let client = kube::Client::try_default().await?;
	reconciler::run(client).await;

	// Controller loop exited (shutdown signal received or fatal error).
	// Log completion so operators can distinguish clean shutdown from crash.
	tracing::warn!("Controller loop terminated, shutting down");

	Ok(())
}
