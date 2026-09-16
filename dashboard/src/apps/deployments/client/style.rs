//! Generated component styles owned by the deployments client.

use reinhardt::pages::style_def;

/// Typed class tokens for deployment inventory, operations, health, and logs.
#[style_def]
pub static STYLES: DeploymentsStyles = style! {
	.alert {
		padding: 0.5rem;
		padding-left: 0.75rem;
		padding-right: 0.75rem;
		border-width: 1px;
		border-style: solid;
		border-radius: 0.375rem;
		font-size: 0.875rem;
		font-weight: 500;
	}
	.alert_error {
		border-color: #fecaca;
		background-color: #fef2f2;
		color: #b91c1c;
	}
	.alert_success {
		border-color: #bbf7d0;
		background-color: #f0fdf4;
		color: #166534;
	}
	.field_error {
		margin-top: 0.25rem;
		font-size: 0.75rem;
		font-weight: 500;
		color: #b91c1c;
	}
	.form_margin {
		margin-top: 0.75rem;
	}
	.form_submit {
		width: 100%;
		min-height: 2.75rem;
	}
	.form_submit_create {
		@media (min-width: 768px) {
			width: auto;
			justify-self: start;
		}
	}
	.form_status {
		margin-top: 0.5rem;
		font-size: 0.75rem;
		color: #625f68;
	}
	.dirty_notice {
		margin-top: 0.5rem;
		font-size: 0.75rem;
		color: brown;
	}
	.delete_confirmation {
		display: flex;
		align-items: flex-start;
		gap: 0.5rem;
		font-size: 0.875rem;
		color: #2b2a30;
	}
	.refetch_notice {
		padding: 0.5rem;
		padding-left: 1rem;
		padding-right: 1rem;
		border-bottom-width: 1px;
		border-bottom-style: solid;
		font-size: 0.75rem;
		font-weight: 500;
	}
	.refetch_warning {
		border-bottom-color: #fde68a;
		background-color: #fffbeb;
		color: brown;
	}
	.refetch_pending {
		border-bottom-color: #dbeafe;
		background-color: #eff6ff;
		color: #1e3a8a;
	}
	.refetch_neutral {
		border-bottom-color: #dbeafe;
		background-color: #eff6ff;
		color: #4f7796;
	}
	.query_error {
		padding: 2rem;
		padding-left: 1rem;
		padding-right: 1rem;
		font-size: 0.875rem;
		font-weight: 500;
		color: #b91c1c;
	}
	.section_title {
		margin-bottom: 0.75rem;
		font-size: 0.875rem;
		font-weight: 600;
		color: #111013;
	}
	.section_gap {
		margin-top: 0.75rem;
	}
	.intro {
		margin-top: 0.25rem;
	}
	.operation_state {
		margin-bottom: 0.75rem;
		font-size: 0.75rem;
	}
	.operation_idle {
		color: #4f7796;
	}
	.operation_pending {
		color: #625f68;
	}
	.operation_error {
		margin-bottom: 0.75rem;
		font-size: 0.75rem;
		font-weight: 500;
		color: #b91c1c;
	}
	.page_layout {
		display: grid;
		gap: 1.5rem;
		@media (min-width: 1024px) {
			grid-template-columns: (1fr, 20rem);
		}
	}
	.content_stack {
		display: grid;
		gap: 1.5rem;
	}
	.divider {
		margin-top: 1rem;
		margin-bottom: 1rem;
		border-top-width: 1px;
		border-top-style: solid;
		border-top-color: #d8d2c3;
	}
	.inventory_scroll {
		overflow-x: auto;
	}
	.inventory_head {
		background-color: #eff6ff;
	}
	.inventory_body {
		background-color: white;
	}
	.inventory_row {
		border-top-width: 1px;
		border-top-style: solid;
		border-top-color: #dbeafe;
	}
	.inventory_id {
		font-family: monospace;
		font-size: 0.75rem;
	}
	.inventory_image {
		max-width: 20rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.project_name {
		font-weight: 600;
		color: #111013;
	}
	.project_empty {
		margin-top: 0.5rem;
		font-size: 0.75rem;
		font-weight: 500;
		color: #4f7796;
	}
	.preview_identity {
		min-width: 0;
		display: grid;
		gap: 0.25rem;
	}
	.preview_name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-weight: 600;
		color: #111013;
	}
	.preview_meta {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 0.75rem;
		font-weight: 500;
		color: #625f68;
	}
	.preview_error {
		margin-top: 0.5rem;
		font-size: 0.75rem;
		font-weight: 500;
		color: brown;
	}
	.preview_empty {
		margin-top: 0.5rem;
		font-size: 0.75rem;
		font-weight: 500;
		color: #4f7796;
	}
	.preview_list {
		display: grid;
		gap: 0.25rem;
		margin-top: 0.5rem;
		font-size: 0.75rem;
	}
	.preview_item {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		column-gap: 0.5rem;
		row-gap: 0.25rem;
	}
	.preview_link {
		font-weight: 600;
		color: #0a4d48;
		text-decoration: underline;
		&:hover {
			color: #0a4d48;
		}
	}
	.cluster_health {
		display: grid;
		gap: 0.5rem;
	}
	.cluster_health_row {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.5rem;
		border-width: 1px;
		border-style: solid;
		border-radius: 0.375rem;
		font-size: 0.875rem;
	}
	.cluster_health_healthy {
		border-color: #bbf7d0;
		background-color: #f0fdf4;
	}
	.cluster_health_unhealthy {
		border-color: #fecaca;
		background-color: #fef2f2;
	}
	.health_name {
		color: #111013;
	}
	.health_muted {
		color: #625f68;
	}
	.health_timestamp {
		margin-left: auto;
		color: #9a96a3;
	}
	.log_viewer {
		max-height: 24rem;
		overflow: auto;
		padding: 0.75rem;
		border-radius: 0.375rem;
		background-color: #111013;
		font-family: monospace;
		font-size: 0.75rem;
		color: #f3f4f6;
		white-space: pre-wrap;
	}
	.log_line {
		display: block;
	}
	.log_line_default {
		color: #f3f4f6;
	}
	.log_line_muted {
		color: #9ca3af;
	}
	.log_line_warning {
		color: #fde68a;
	}
	.log_line_error {
		color: #fca5a5;
	}
};

/// Select the generated color token for a log level.
#[cfg(any(wasm, test))]
pub(super) fn log_level_class(level: &str) -> reinhardt::pages::prelude::ClassToken {
	match level {
		"error" => STYLES.log_line_error(),
		"warn" => STYLES.log_line_warning(),
		"debug" => STYLES.log_line_muted(),
		_ => STYLES.log_line_default(),
	}
}

/// Combine the log line structure with its level-specific color.
#[cfg(any(wasm, test))]
pub(super) fn log_line_class(level: &str) -> reinhardt::pages::style::ClassList {
	STYLES.log_line() + log_level_class(level)
}

#[cfg(test)]
mod tests {
	use super::{STYLES, log_level_class, log_line_class};
	use rstest::rstest;

	#[rstest]
	fn test_level_class_maps_known_levels() {
		// Act
		let error = log_level_class("error");
		let warning = log_level_class("warn");
		let muted = log_level_class("debug");
		let default = log_level_class("unknown");

		// Assert
		assert_eq!(error.as_str(), STYLES.log_line_error().as_str());
		assert_eq!(warning.as_str(), STYLES.log_line_warning().as_str());
		assert_eq!(muted.as_str(), STYLES.log_line_muted().as_str());
		assert_eq!(default.as_str(), STYLES.log_line_default().as_str());
	}

	#[rstest]
	fn test_log_line_class_composes_generated_base_and_level_tokens() {
		// Act
		let class = log_line_class("error");

		// Assert
		assert_eq!(
			class.as_str(),
			(STYLES.log_line() + STYLES.log_line_error()).as_str()
		);
	}
}
