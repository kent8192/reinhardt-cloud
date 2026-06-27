//! Preview status read service for Dashboard project rows.

use crate::apps::deployments::server_fn::{ProjectPreviewSummary, ProjectSourceKind};

/// Input row used to resolve preview status for a Dashboard project surface.
#[derive(Debug, Clone)]
pub(crate) struct PreviewProjectInput {
	pub deployment_id: i64,
	pub github_project_id: Option<i64>,
	pub project_name: String,
	pub display_name: String,
	pub production_branch: Option<String>,
	pub source_kind: ProjectSourceKind,
}

const PREVIEW_STATUS_UNAVAILABLE: &str =
	"Preview status is unavailable until cluster agent telemetry reports Project status";

/// Loads preview status for one project, keeping read errors local to the row.
pub(crate) async fn load_preview_summary(
	input: PreviewProjectInput,
	_default_namespace: &str,
) -> ProjectPreviewSummary {
	ProjectPreviewSummary {
		deployment_id: input.deployment_id,
		github_project_id: input.github_project_id,
		project_name: input.project_name,
		display_name: input.display_name,
		production_branch: input.production_branch,
		source_kind: input.source_kind,
		previews: Vec::new(),
		preview_error: Some(PREVIEW_STATUS_UNAVAILABLE.to_string()),
	}
}

#[cfg(test)]
mod tests {
	use rstest::rstest;

	use super::{PREVIEW_STATUS_UNAVAILABLE, PreviewProjectInput, load_preview_summary};
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
			Some(PREVIEW_STATUS_UNAVAILABLE)
		);
	}
}
