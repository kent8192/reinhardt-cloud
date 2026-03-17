//! Nuages Kubernetes operator for managing `ReinhardtApp` resources.

mod error;
mod inference;
mod reconciler;
mod resources;

use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	fmt()
		.with_env_filter(
			EnvFilter::from_default_env().add_directive("nuages_operator=info".parse()?),
		)
		.init();

	tracing::info!("Starting nuages operator");

	let client = kube::Client::try_default().await?;
	reconciler::run(client).await;

	Ok(())
}
