//! Shared preview environment rendering for Dashboard project surfaces.

use reinhardt::pages::component::Page;
use reinhardt::pages::page;

use crate::apps::deployments::client::style::STYLES;
use crate::apps::deployments::server_fn::{
	PreviewSummary, ProjectPreviewSummary, ProjectSourceKind,
};

/// Renders the primary and secondary project identity used by preview surfaces.
pub fn render_project_identity(summary: &ProjectPreviewSummary) -> Page {
	let display_name = summary.display_name.clone();
	let secondary = match summary.source_kind {
		ProjectSourceKind::GitHub => summary
			.production_branch
			.as_ref()
			.map(|branch| format!("Project: {} / production: {branch}", summary.project_name))
			.unwrap_or_else(|| format!("Project: {}", summary.project_name)),
		ProjectSourceKind::Manual => "Manual Project".to_string(),
	};
	page!({
		div {
			class: STYLES.preview_identity(),
			div {
				class: STYLES.preview_name(),
				{ display_name }
			}
			div {
				class: STYLES.preview_meta(),
				{ secondary }
			}
		}
	})
}

/// Renders preview state for one parent Project.
pub fn render_preview_list(summary: &ProjectPreviewSummary) -> Page {
	if let Some(error) = summary.preview_error.as_ref() {
		let error = error.clone();
		return page!({
			div {
				class: STYLES.preview_error(),
				{ error }
			}
		});
	}
	if summary.previews.is_empty() {
		return page!({
			div {
				class: STYLES.preview_empty(),
				"No active previews"
			}
		});
	}
	let previews = summary.previews.clone();
	page!({
		ul {
			class: STYLES.preview_list(),
			{ previews
			.iter()
			.map(self::render_preview_item)
			.collect::<Vec<_>>() }
		}
	})
}

fn render_preview_item(preview: &PreviewSummary) -> Page {
	let label = format!("#{} {}", preview.pr_number, preview.name);
	let meta = preview_meta(preview);
	match preview.url.as_ref() {
		Some(url) => {
			let url = url.clone();
			page!({
				li {
					class: STYLES.preview_item(),
					a {
						class: STYLES.preview_link(),
						href: url,
						target: "_blank",
						rel: "noreferrer",
						{ label }
					}
					span {
						class: STYLES.preview_meta(),
						{ meta }
					}
				}
			})
		}
		None => page!({
			li {
				class: STYLES.preview_item(),
				span {
					class: STYLES.preview_name(),
					{ label }
				}
				span {
					class: STYLES.preview_meta(),
					{ meta }
				}
			}
		}),
	}
}

fn preview_meta(preview: &PreviewSummary) -> String {
	match (preview.phase.as_deref(), preview.ready_replicas) {
		(Some(phase), Some(replicas)) => format!("{phase} / {replicas} ready"),
		(Some(phase), None) => phase.to_string(),
		(None, Some(replicas)) => format!("{replicas} ready"),
		(None, None) => "status pending".to_string(),
	}
}
