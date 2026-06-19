//! Preview status read service for Dashboard project rows.

use crate::apps::deployments::server_fn::{
	ProjectPreviewSummary, ProjectSourceKind, preview_project_ref_from_yaml,
	preview_summary_from_status,
};

/// Input row used to resolve preview status for a Dashboard project surface.
#[derive(Debug, Clone)]
pub(crate) struct PreviewProjectInput {
	pub deployment_id: i64,
	pub github_project_id: Option<i64>,
	pub project_name: String,
	pub display_name: String,
	pub production_branch: Option<String>,
	pub source_kind: ProjectSourceKind,
	pub project_yaml: Option<String>,
}

/// Builds a Kubernetes client for the namespace that owns a parent Project.
pub(crate) async fn kube_client_for_namespace(
	namespace: &str,
) -> Result<reinhardt_cloud_k8s::KubeClient, String> {
	Ok(
		match reinhardt_cloud_k8s::KubeClient::in_cluster(namespace).await {
			Ok(client) => client,
			Err(in_cluster_error) => {
				let in_cluster_error = in_cluster_error.to_string();
				reinhardt_cloud_k8s::KubeClient::from_kubeconfig(namespace)
					.await
					.map_err(|kubeconfig_error| {
						let kubeconfig_error = kubeconfig_error.to_string();
						format!(
							"Failed to build Kubernetes client from in-cluster config ({}) or kubeconfig ({})",
							in_cluster_error, kubeconfig_error
						)
					})?
			}
		},
	)
}

/// Loads preview status for one project, keeping read errors local to the row.
pub(crate) async fn load_preview_summary(
	input: PreviewProjectInput,
	default_namespace: &str,
) -> ProjectPreviewSummary {
	let project_ref = match preview_project_ref_from_yaml(
		&input.project_name,
		input.project_yaml.as_deref(),
		default_namespace,
	) {
		Ok(project_ref) => project_ref,
		Err(err) => return summary_with_error(input, err),
	};

	let previews = match read_project_previews(&project_ref.namespace, &project_ref.name).await {
		Ok(previews) => previews,
		Err(err) => return summary_with_error(input, err),
	};

	ProjectPreviewSummary {
		deployment_id: input.deployment_id,
		github_project_id: input.github_project_id,
		project_name: input.project_name,
		display_name: input.display_name,
		production_branch: input.production_branch,
		source_kind: input.source_kind,
		previews: previews
			.into_iter()
			.map(preview_summary_from_status)
			.collect(),
		preview_error: None,
	}
}

fn summary_with_error(input: PreviewProjectInput, err: String) -> ProjectPreviewSummary {
	ProjectPreviewSummary {
		deployment_id: input.deployment_id,
		github_project_id: input.github_project_id,
		project_name: input.project_name,
		display_name: input.display_name,
		production_branch: input.production_branch,
		source_kind: input.source_kind,
		previews: Vec::new(),
		preview_error: Some(err),
	}
}

async fn read_project_previews(
	namespace: &str,
	name: &str,
) -> Result<Vec<reinhardt_cloud_types::crd::PreviewStatus>, String> {
	let client = kube_client_for_namespace(namespace).await?;
	let project = reinhardt_cloud_k8s::resources::get_project(&client, namespace, name)
		.await
		.map_err(|e| e.to_string())?;
	Ok(project
		.status
		.map(|status| status.previews)
		.unwrap_or_default())
}

#[cfg(test)]
mod tests {
	use rstest::rstest;

	use super::{PreviewProjectInput, load_preview_summary};
	use crate::apps::deployments::server_fn::ProjectSourceKind;

	#[rstest]
	#[tokio::test]
	async fn load_preview_summary_returns_row_error_for_missing_manifest() {
		// Arrange
		let input = PreviewProjectInput {
			deployment_id: 7,
			github_project_id: None,
			project_name: "api".to_string(),
			display_name: "api".to_string(),
			production_branch: None,
			source_kind: ProjectSourceKind::Manual,
			project_yaml: None,
		};

		// Act
		let summary = load_preview_summary(input, "default").await;

		// Assert
		assert_eq!(summary.deployment_id, 7);
		assert_eq!(summary.project_name, "api");
		assert_eq!(summary.display_name, "api");
		assert_eq!(summary.source_kind, ProjectSourceKind::Manual);
		assert!(summary.previews.is_empty());
		assert_eq!(
			summary.preview_error.as_deref(),
			Some("Project manifest is not available")
		);
	}
}
