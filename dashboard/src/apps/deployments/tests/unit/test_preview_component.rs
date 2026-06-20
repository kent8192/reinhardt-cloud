//! Tests for deployment preview render helpers.

#[cfg(test)]
mod tests {
	use rstest::rstest;

	use crate::apps::deployments::client::components::preview_list::{
		render_preview_list, render_project_identity,
	};
	use crate::apps::deployments::server_fn::{
		PreviewSummary, ProjectPreviewSummary, ProjectSourceKind,
	};

	#[rstest]
	fn render_project_identity_uses_github_repository_display_name() {
		// Arrange
		let summary = github_summary();

		// Act
		let html = render_project_identity(&summary).render_to_string();

		// Assert
		assert_eq!(
			html,
			"<div class=\"min-w-0 space-y-1\"><div class=\"truncate font-semibold text-ink-950\">kent8192/reinhardt-cloud</div><div class=\"truncate text-xs font-medium text-ink-600\">Project: reinhardt-cloud / production: main</div></div>"
		);
	}

	#[rstest]
	fn render_project_identity_uses_manual_project_label() {
		// Arrange
		let summary = manual_summary();

		// Act
		let html = render_project_identity(&summary).render_to_string();

		// Assert
		assert_eq!(
			html,
			"<div class=\"min-w-0 space-y-1\"><div class=\"truncate font-semibold text-ink-950\">api</div><div class=\"truncate text-xs font-medium text-ink-600\">Manual Project</div></div>"
		);
	}

	#[rstest]
	fn render_preview_list_outputs_project_scoped_preview_rows() {
		// Arrange
		let summary = github_summary();

		// Act
		let html = render_preview_list(&summary).render_to_string();

		// Assert
		assert_eq!(
			html,
			"<ul class=\"mt-2 space-y-1 text-xs\"><li class=\"flex flex-wrap items-center gap-x-2 gap-y-1\"><a class=\"font-semibold text-control-700 underline underline-offset-2 hover:text-control-900\" href=\"https://preview.example.com/pr-42\" target=\"_blank\" rel=\"noreferrer\">#42 reinhardt-cloud-pr-42</a><span class=\"text-cloud-500\">running / 1 ready</span></li></ul>"
		);
	}

	#[rstest]
	fn render_preview_list_outputs_empty_state() {
		// Arrange
		let summary = manual_summary();

		// Act
		let html = render_preview_list(&summary).render_to_string();

		// Assert
		assert_eq!(
			html,
			"<div class=\"mt-2 text-xs font-medium text-cloud-500\">No active previews</div>"
		);
	}

	#[rstest]
	fn render_preview_list_outputs_row_error_state() {
		// Arrange
		let mut summary = manual_summary();
		summary.preview_error = Some("Project manifest is not available".to_string());

		// Act
		let html = render_preview_list(&summary).render_to_string();

		// Assert
		assert_eq!(
			html,
			"<div class=\"mt-2 text-xs font-medium text-amber-700\">Project manifest is not available</div>"
		);
	}

	fn github_summary() -> ProjectPreviewSummary {
		ProjectPreviewSummary {
			deployment_id: 10,
			github_project_id: Some(20),
			project_name: "reinhardt-cloud".to_string(),
			display_name: "kent8192/reinhardt-cloud".to_string(),
			production_branch: Some("main".to_string()),
			source_kind: ProjectSourceKind::GitHub,
			previews: vec![PreviewSummary {
				name: "reinhardt-cloud-pr-42".to_string(),
				pr_number: "42".to_string(),
				url: Some("https://preview.example.com/pr-42".to_string()),
				phase: Some("running".to_string()),
				ready_replicas: Some(1),
				last_activity: None,
			}],
			preview_error: None,
		}
	}

	fn manual_summary() -> ProjectPreviewSummary {
		ProjectPreviewSummary {
			deployment_id: 11,
			github_project_id: None,
			project_name: "api".to_string(),
			display_name: "api".to_string(),
			production_branch: None,
			source_kind: ProjectSourceKind::Manual,
			previews: Vec::new(),
			preview_error: None,
		}
	}
}
