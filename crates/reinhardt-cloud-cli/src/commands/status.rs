//! Status command: checks the deployment status.

use std::process::Command;

use clap::Args;

use crate::client::ReinhardtCloudClient;

/// Check deployment status.
#[derive(Debug, Args)]
pub(crate) struct StatusArgs {
	/// Application name to check
	#[arg(short, long)]
	pub name: Option<String>,
}

/// Executes the status command.
///
/// Tries the dashboard API first; falls back to `kubectl` if the API is unreachable.
pub(crate) async fn execute(
	args: &StatusArgs,
	client: &ReinhardtCloudClient,
) -> Result<(), Box<dyn std::error::Error>> {
	let app_name = args.name.as_deref().unwrap_or("default-app");
	println!("Checking status of {app_name}...");

	// Try the dashboard API first
	match client.get_status(app_name).await {
		Ok(status) => {
			let formatted =
				serde_json::to_string_pretty(&status).unwrap_or_else(|_| status.to_string());
			println!("{formatted}");
			Ok(())
		}
		Err(e) => {
			tracing::warn!("Dashboard API unreachable, falling back to kubectl: {e}");
			eprintln!("Dashboard API unavailable, falling back to kubectl...");
			kubectl_status(app_name)
		}
	}
}

/// Queries deployment status directly via `kubectl`.
fn kubectl_status(app_name: &str) -> Result<(), Box<dyn std::error::Error>> {
	let output = Command::new("kubectl")
		.args(["get", "reinhardtapp", app_name, "-o", "json"])
		.output()
		.map_err(|e| format!("failed to run kubectl: {e}"))?;

	if output.status.success() {
		let stdout = String::from_utf8_lossy(&output.stdout);
		let value: serde_json::Value = serde_json::from_str(&stdout)
			.map_err(|e| format!("failed to parse kubectl output: {e}"))?;

		// Extract and display relevant status fields
		if let Some(status) = value.get("status") {
			let formatted =
				serde_json::to_string_pretty(status).unwrap_or_else(|_| status.to_string());
			println!("{formatted}");
		} else {
			println!("No status field found in CRD.");
			let formatted =
				serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
			println!("{formatted}");
		}
		Ok(())
	} else {
		let stderr = String::from_utf8_lossy(&output.stderr);
		Err(format!("kubectl get reinhardtapp failed: {stderr}").into())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_status_args_default_name() {
		// Arrange
		let args = StatusArgs { name: None };

		// Act
		let name = args.name.as_deref().unwrap_or("default-app");

		// Assert
		assert_eq!(name, "default-app");
	}

	#[rstest]
	fn test_status_args_custom_name() {
		// Arrange
		let args = StatusArgs {
			name: Some("my-app".to_string()),
		};

		// Act
		let name = args.name.as_deref().unwrap_or("default-app");

		// Assert
		assert_eq!(name, "my-app");
	}
}
