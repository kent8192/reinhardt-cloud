//! Status command: checks the deployment status via kubectl.

use clap::Args;
use colored::Colorize;
use serde::Deserialize;
use std::process::Command;

use crate::client::ReinhardtCloudClient;

/// Check deployment status.
#[derive(Debug, Args)]
pub(crate) struct StatusArgs {
	/// Application name to check
	#[arg(short, long)]
	pub name: Option<String>,

	/// Kubernetes namespace
	#[arg(long, default_value = "default")]
	pub namespace: String,

	/// Target cluster name
	#[arg(long)]
	pub cluster: Option<String>,
}

/// Condition from a Kubernetes resource status.
#[derive(Debug, Deserialize)]
struct StatusCondition {
	#[serde(rename = "type")]
	condition_type: String,
	status: String,
	#[serde(default)]
	message: Option<String>,
}

/// Partial representation of a ReinhardtApp CRD for status display.
#[derive(Debug, Deserialize)]
struct ReinhardtAppResource {
	metadata: ResourceMetadata,
	spec: ResourceSpec,
	#[serde(default)]
	status: Option<ResourceStatus>,
}

/// Metadata section of the CRD.
#[derive(Debug, Deserialize)]
struct ResourceMetadata {
	name: String,
	namespace: Option<String>,
}

/// Spec section of the CRD.
#[derive(Debug, Deserialize)]
struct ResourceSpec {
	image: String,
	#[serde(default)]
	replicas: Option<i32>,
}

/// Status section of the CRD.
#[derive(Debug, Deserialize)]
struct ResourceStatus {
	#[serde(default)]
	conditions: Vec<StatusCondition>,
	#[serde(default, rename = "readyReplicas")]
	ready_replicas: Option<i32>,
}

/// Queries kubectl for a ReinhardtApp resource and returns the JSON output.
fn query_kubectl_status(
	name: &str,
	namespace: &str,
	cluster: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
	let mut cmd = Command::new("kubectl");
	cmd.args(["get", "reinhardtapp", name, "-n", namespace, "-o", "json"]);

	if let Some(ctx) = cluster {
		cmd.args(["--context", ctx]);
	}

	let output = cmd
		.output()
		.map_err(|e| format!("Failed to run kubectl (is it installed?): {e}"))?;

	if output.status.success() {
		let stdout = String::from_utf8(output.stdout)
			.map_err(|e| format!("Invalid UTF-8 in kubectl output: {e}"))?;
		Ok(stdout)
	} else {
		let stderr = String::from_utf8_lossy(&output.stderr);
		Err(format!("kubectl get failed: {stderr}").into())
	}
}

/// Determines the overall status label from conditions.
fn determine_status_label(conditions: &[StatusCondition]) -> &'static str {
	for cond in conditions {
		if cond.condition_type == "Ready" && cond.status == "True" {
			return "Ready";
		}
		if cond.condition_type == "Degraded" && cond.status == "True" {
			return "Degraded";
		}
	}
	for cond in conditions {
		if cond.condition_type == "Progressing" && cond.status == "True" {
			return "Progressing";
		}
	}
	"Unknown"
}

/// Returns a color-coded status string.
fn colorize_status(label: &str) -> String {
	match label {
		"Ready" => label.green().bold().to_string(),
		"Progressing" => label.yellow().bold().to_string(),
		"Degraded" => label.red().bold().to_string(),
		_ => label.dimmed().to_string(),
	}
}

/// Renders the status output for a ReinhardtApp resource.
fn display_status(resource: &ReinhardtAppResource) {
	let name = &resource.metadata.name;
	let namespace = resource.metadata.namespace.as_deref().unwrap_or("default");
	let image = &resource.spec.image;
	let replicas = resource.spec.replicas.unwrap_or(1);

	println!("Application:  {name}");
	println!("Namespace:    {namespace}");
	println!("Image:        {image}");
	println!("Replicas:     {replicas}");

	if let Some(ref status) = resource.status {
		if let Some(ready) = status.ready_replicas {
			println!("Ready:        {ready}/{replicas}");
		}

		let label = determine_status_label(&status.conditions);
		println!("Status:       {}", colorize_status(label));

		if !status.conditions.is_empty() {
			println!("\nConditions:");
			for cond in &status.conditions {
				let status_indicator = if cond.status == "True" { "+" } else { "-" };
				let msg = cond.message.as_deref().unwrap_or("");
				println!(
					"  [{status_indicator}] {}: {}{}",
					cond.condition_type,
					cond.status,
					if msg.is_empty() {
						String::new()
					} else {
						format!(" — {msg}")
					}
				);
			}
		}
	} else {
		println!("Status:       {}", colorize_status("Unknown"));
		println!("  (no status reported yet)");
	}
}

/// Executes the status command.
pub(crate) async fn execute(
	args: &StatusArgs,
	_client: &ReinhardtCloudClient,
) -> Result<(), Box<dyn std::error::Error>> {
	let app_name = args.name.as_deref().unwrap_or("default-app");
	println!("Checking status of {app_name}...\n");

	let json_output = query_kubectl_status(app_name, &args.namespace, args.cluster.as_deref())?;

	let resource: ReinhardtAppResource = serde_json::from_str(&json_output)
		.map_err(|e| format!("Failed to parse kubectl JSON output: {e}"))?;

	display_status(&resource);

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	#[case("Ready", "True", "Ready")]
	#[case("Degraded", "True", "Degraded")]
	#[case("Progressing", "True", "Progressing")]
	fn test_determine_status_label_single_condition(
		#[case] condition_type: &str,
		#[case] status: &str,
		#[case] expected: &str,
	) {
		// Arrange
		let conditions = vec![StatusCondition {
			condition_type: condition_type.to_string(),
			status: status.to_string(),
			message: None,
		}];

		// Act
		let label = determine_status_label(&conditions);

		// Assert
		assert_eq!(label, expected);
	}

	#[rstest]
	fn test_determine_status_label_ready_takes_precedence() {
		// Arrange
		let conditions = vec![
			StatusCondition {
				condition_type: "Progressing".to_string(),
				status: "True".to_string(),
				message: None,
			},
			StatusCondition {
				condition_type: "Ready".to_string(),
				status: "True".to_string(),
				message: None,
			},
		];

		// Act
		let label = determine_status_label(&conditions);

		// Assert
		assert_eq!(label, "Ready");
	}

	#[rstest]
	fn test_determine_status_label_degraded_takes_precedence_over_progressing() {
		// Arrange
		let conditions = vec![
			StatusCondition {
				condition_type: "Progressing".to_string(),
				status: "True".to_string(),
				message: None,
			},
			StatusCondition {
				condition_type: "Degraded".to_string(),
				status: "True".to_string(),
				message: None,
			},
		];

		// Act
		let label = determine_status_label(&conditions);

		// Assert
		assert_eq!(label, "Degraded");
	}

	#[rstest]
	fn test_determine_status_label_empty_conditions() {
		// Arrange
		let conditions: Vec<StatusCondition> = vec![];

		// Act
		let label = determine_status_label(&conditions);

		// Assert
		assert_eq!(label, "Unknown");
	}

	#[rstest]
	fn test_determine_status_label_false_ready_is_not_ready() {
		// Arrange
		let conditions = vec![StatusCondition {
			condition_type: "Ready".to_string(),
			status: "False".to_string(),
			message: Some("not yet".to_string()),
		}];

		// Act
		let label = determine_status_label(&conditions);

		// Assert
		assert_eq!(label, "Unknown");
	}

	#[rstest]
	fn test_colorize_status_ready_contains_text() {
		// Arrange & Act
		let output = colorize_status("Ready");

		// Assert: the colored output should contain the "Ready" text
		assert!(output.contains("Ready"));
	}

	#[rstest]
	fn test_colorize_status_unknown_contains_text() {
		// Arrange & Act
		let output = colorize_status("Unknown");

		// Assert
		assert!(output.contains("Unknown"));
	}

	#[rstest]
	fn test_parse_reinhardt_app_resource_full() {
		// Arrange
		let json = r#"{
			"metadata": {
				"name": "my-app",
				"namespace": "production"
			},
			"spec": {
				"image": "my-app:v2",
				"replicas": 3
			},
			"status": {
				"conditions": [
					{
						"type": "Ready",
						"status": "True",
						"message": "All replicas are ready"
					},
					{
						"type": "Progressing",
						"status": "False"
					}
				],
				"readyReplicas": 3
			}
		}"#;

		// Act
		let resource: ReinhardtAppResource = serde_json::from_str(json).unwrap();

		// Assert
		assert_eq!(resource.metadata.name, "my-app");
		assert_eq!(resource.metadata.namespace.as_deref(), Some("production"));
		assert_eq!(resource.spec.image, "my-app:v2");
		assert_eq!(resource.spec.replicas, Some(3));
		let status = resource.status.unwrap();
		assert_eq!(status.conditions.len(), 2);
		assert_eq!(status.ready_replicas, Some(3));
		assert_eq!(status.conditions[0].condition_type, "Ready");
		assert_eq!(
			status.conditions[0].message.as_deref(),
			Some("All replicas are ready")
		);
	}

	#[rstest]
	fn test_parse_reinhardt_app_resource_without_status() {
		// Arrange
		let json = r#"{
			"metadata": {
				"name": "new-app",
				"namespace": "staging"
			},
			"spec": {
				"image": "new-app:v1"
			}
		}"#;

		// Act
		let resource: ReinhardtAppResource = serde_json::from_str(json).unwrap();

		// Assert
		assert_eq!(resource.metadata.name, "new-app");
		assert_eq!(resource.spec.image, "new-app:v1");
		assert!(resource.spec.replicas.is_none());
		assert!(resource.status.is_none());
	}

	#[rstest]
	fn test_query_kubectl_status_fails_without_kubectl() {
		// Arrange & Act
		let result = query_kubectl_status("test-app", "default", None);

		// Assert: should fail because kubectl is not available in test env
		assert!(result.is_err());
		let err = result.unwrap_err().to_string();
		assert!(
			err.contains("kubectl"),
			"expected kubectl-related error, got: {err}"
		);
	}
}
