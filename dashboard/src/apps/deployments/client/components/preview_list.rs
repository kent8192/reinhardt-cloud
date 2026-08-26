//! Shared preview environment rendering for Dashboard project surfaces.

use reinhardt::pages::component::Page;
use reinhardt::pages::page;

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
			class: "min-w-0 space-y-1",
			div {
				class: "truncate font-semibold text-ink-950",
				{ display_name }
			}
			div {
				class: "truncate text-xs font-medium text-ink-600",
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
				class: "mt-2 text-xs font-medium text-amber-700",
				{ error }
			}
		});
	}
	if summary.previews.is_empty() {
		return page!({
			div {
				class: "mt-2 text-xs font-medium text-cloud-500",
				"No active previews"
			}
		});
	}
	let previews = summary.previews.clone();
	page!({
		ul {
			class: "mt-2 space-y-1 text-xs",
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
					class: "flex flex-wrap items-center gap-x-2 gap-y-1",
					a {
						class: "font-semibold text-control-700 underline underline-offset-2 hover:text-control-900",
						href: url,
						target: "_blank",
						rel: "noreferrer",
						{ label }
					}
					span {
						class: "text-cloud-500",
						{ meta }
					}
				}
			})
		}
		None => page!({
			li {
				class: "flex flex-wrap items-center gap-x-2 gap-y-1",
				span {
					class: "font-semibold text-ink-950",
					{ label }
				}
				span {
					class: "text-cloud-500",
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
