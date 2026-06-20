//! Tests for deployment preview status service mapping.

#[cfg(test)]
mod tests {
	use rstest::rstest;

	use crate::apps::deployments::server_fn::ProjectSourceKind;
	use crate::apps::deployments::services::preview_status::{
		PreviewProjectInput, load_preview_summary,
	};

	#[rstest]
	#[tokio::test]
	async fn load_preview_summary_keeps_missing_manifest_error_on_row() {
		// Arrange
		let input = PreviewProjectInput {
			deployment_id: 7,
			github_project_id: None,
			project_name: "api".to_string(),
			display_name: "api".to_string(),
			production_branch: None,
			source_kind: ProjectSourceKind::Manual,
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
			Some(
				"Preview status is unavailable until cluster agent telemetry reports Project status"
			)
		);
	}
}
