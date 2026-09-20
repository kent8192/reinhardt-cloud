//! Generated component styles owned by the GitHub client.

use reinhardt::pages::style_def;

/// Typed class tokens for GitHub onboarding, imports, and repository inventory.
#[style_def]
pub static STYLES: GithubStyles = style! {
	.alert {
		padding: 0.5rem;
		padding-left: 0.75rem;
		padding-right: 0.75rem;
		border-width: 1px;
		border-style: solid;
		border-color: #fecaca;
		border-radius: 0.375rem;
		background-color: #fef2f2;
		font-size: 0.875rem;
		font-weight: 500;
		color: #b91c1c;
	}
	.refetch_notice {
		margin-bottom: 0.75rem;
		padding: 0.5rem;
		padding-left: 0.75rem;
		padding-right: 0.75rem;
		border-width: 1px;
		border-style: solid;
		border-radius: 0.375rem;
		font-size: 0.75rem;
		font-weight: 500;
	}
	.refetch_warning {
		border-color: #fde68a;
		background-color: #fffbeb;
		color: brown;
	}
	.refetch_pending {
		border-color: #dbeafe;
		background-color: #eff6ff;
		color: #4f7796;
	}
	.field_error {
		margin-top: 0.25rem;
		font-size: 0.75rem;
		font-weight: 500;
		color: #b91c1c;
	}
	.form_submit {
		width: 100%;
		min-height: 2.75rem;
		@media (min-width: 768px) {
			width: auto;
			justify-self: start;
		}
	}
	.page_intro {
		margin-top: 0.25rem;
	}
	.page_layout {
		display: grid;
		gap: 1.5rem;
		@media (min-width: 1024px) {
			grid-template-columns: (1fr, 22.5rem);
		}
	}
	.content_stack {
		display: grid;
		gap: 1.5rem;
	}
	.panel_body {
		padding: 1rem;
	}
	.project_grid {
		display: grid;
		gap: 0.75rem;
		@media (min-width: 1280px) {
			grid-template-columns: unchecked_fn!(repeat(2, minmax(0, 1fr)));
		}
	}
	.project_card {
		padding: 1rem;
		border-width: 1px;
		border-style: solid;
		border-color: #d8d2c3;
		border-radius: 0.375rem;
		background-color: white;
	}
	.query_notice {
		padding: 2rem;
		padding-left: 1rem;
		padding-right: 1rem;
		font-size: 0.875rem;
		font-weight: 500;
	}
	.query_warning {
		color: brown;
	}
	.query_error {
		color: #b91c1c;
	}
	.inventory_head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
	}
	.github_badge {
		display: inline-flex;
		padding: 0.25rem;
		padding-left: 0.625rem;
		padding-right: 0.625rem;
		border-radius: 9999px;
		background-color: #e0f2f0;
		font-size: 0.6875rem;
		font-weight: 700;
		color: #0a4d48;
	}
	.inventory_scroll {
		overflow-x: auto;
	}
	.inventory_header {
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
	.repository_name {
		font-weight: 600;
		color: #111013;
	}
	.repository_visibility {
		margin-top: 0.125rem;
		font-size: 0.75rem;
		font-weight: 500;
		color: #625f68;
	}
	.repository_state {
		display: inline-flex;
		padding: 0.125rem;
		padding-left: 0.625rem;
		padding-right: 0.625rem;
		border-radius: 9999px;
		font-size: 0.75rem;
		font-weight: 600;
	}
	.repository_state_imported {
		background-color: #e0f2f0;
		color: #0a4d48;
	}
	.repository_state_available {
		background-color: #e9e6dd;
		color: #625f68;
	}
	.onboarding_action {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		@media (min-width: 640px) {
			flex-direction: row;
			align-items: center;
			justify-content: space-between;
		}
	}
	.onboarding_button {
		font-size: 0.75rem;
	}
	.aside_title {
		margin-bottom: 0.75rem;
		font-size: 0.875rem;
		font-weight: 600;
		color: #111013;
	}
	.import_selection {
		display: grid;
		gap: 0.5rem;
		margin-bottom: 1rem;
		padding: 0.75rem;
		border-width: 1px;
		border-style: solid;
		border-color: #a6ddd8;
		border-radius: 0.375rem;
		background-color: #f0fdfa;
		font-size: 0.875rem;
	}
	.import_selection_row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
	}
	.import_selection_label {
		font-size: 0.75rem;
		font-weight: 700;
		text-transform: uppercase;
		color: #625f68;
	}
	.import_selection_value {
		font-family: monospace;
		font-size: 0.75rem;
		font-weight: 600;
		color: #111013;
	}
	.import_selection_name {
		overflow: hidden;
		font-size: 0.75rem;
		font-weight: 600;
		color: #111013;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.import_pending {
		margin-bottom: 0.75rem;
		font-size: 0.75rem;
		color: #4f7796;
	}
	.import_error {
		margin-bottom: 0.75rem;
		font-size: 0.75rem;
		font-weight: 500;
		color: #b91c1c;
	}
	.action_status {
		margin-top: 0.5rem;
		font-size: 0.875rem;
		color: #4f7796;
	}
	.cluster_list {
		display: grid;
		gap: 0.5rem;
		font-size: 0.875rem;
	}
	.cluster_empty {
		color: #4f7796;
	}
	.cluster_error {
		color: #b91c1c;
	}
	.cluster_card {
		padding: 0.5rem;
		padding-left: 0.75rem;
		padding-right: 0.75rem;
		border-width: 1px;
		border-style: solid;
		border-color: #d8d2c3;
		border-radius: 0.375rem;
		background-color: white;
	}
	.cluster_card_head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 0.75rem;
	}
	.cluster_card_body {
		min-width: 0;
	}
	.cluster_name {
		overflow: hidden;
		font-weight: 600;
		color: #111013;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.cluster_id {
		margin-top: 0.125rem;
		font-family: monospace;
		font-size: 0.75rem;
		color: #625f68;
	}
	.cluster_url {
		font-family: monospace;
		font-size: 0.75rem;
		color: #4f7796;
	}
};
