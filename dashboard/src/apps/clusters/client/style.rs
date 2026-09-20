//! Generated component styles owned by the clusters client.

use reinhardt::pages::style_def;

/// Typed class tokens for cluster inventory and operation views.
#[style_def]
pub static STYLES: ClustersStyles = style! {
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
	.refresh_notice {
		padding: 0.5rem;
		padding-left: 1rem;
		padding-right: 1rem;
		border-bottom-width: 1px;
		border-bottom-style: solid;
		font-size: 0.75rem;
		font-weight: 500;
	}
	.refresh_warning {
		border-bottom-color: #fde68a;
		background-color: #fffbeb;
		color: brown;
	}
	.refresh_pending {
		border-bottom-color: #d8d2c3;
		background-color: #f6f5f2;
		color: #625f68;
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
	.form_help {
		margin-top: 0.25rem;
		font-size: 0.75rem;
		color: #625f68;
	}
	.dirty_notice {
		margin-top: 0.5rem;
		font-size: 0.75rem;
		color: brown;
	}
	.action_status {
		margin-top: 0.5rem;
		font-size: 0.75rem;
		color: #625f68;
	}
	.form_submit {
		width: 100%;
		min-height: 2.75rem;
	}
	.create_submit {
		@media (min-width: 768px) {
			width: auto;
			justify-self: start;
		}
	}
	.confirmation_field {
		display: flex;
		align-items: flex-start;
		gap: 0.5rem;
		font-size: 0.875rem;
		color: #2b2a30;
	}
	.token_notice {
		margin-top: 0.75rem;
		padding: 0.75rem;
		border-width: 1px;
		border-style: solid;
		border-color: #fcd34d;
		border-radius: 0.375rem;
		background-color: #fffbeb;
		font-size: 0.875rem;
		color: #78350f;
	}
	.token_title {
		font-weight: 600;
	}
	.token_message {
		margin-top: 0.25rem;
	}
	.token_value {
		display: block;
		margin-top: 0.5rem;
		padding: 0.25rem;
		padding-left: 0.5rem;
		padding-right: 0.5rem;
		border-radius: 0.25rem;
		background-color: white;
		font-family: monospace;
		font-size: 0.75rem;
		white-space: pre-wrap;
		word-break: break-all;
	}
	.token_dismiss {
		min-height: 2.5rem;
		margin-top: 0.75rem;
	}
	.inventory_scroll {
		overflow-x: auto;
	}
	.inventory_head {
		background-color: #f6f5f2;
	}
	.inventory_body {
		background-color: white;
	}
	.inventory_row {
		border-top-width: 1px;
		border-top-style: solid;
		border-top-color: #e9e6dd;
	}
	.inventory_id {
		font-family: monospace;
		font-size: 0.75rem;
	}
	.inventory_name {
		font-weight: 600;
		color: #111013;
	}
	.cluster_badge {
		display: inline-flex;
		padding: 0.125rem;
		padding-left: 0.5rem;
		padding-right: 0.5rem;
		border-radius: 9999px;
		font-size: 0.75rem;
		font-weight: 600;
	}
	.cluster_badge_active {
		background-color: #e0f2f0;
		color: #0a4d48;
	}
	.cluster_badge_inactive {
		background-color: #e9e6dd;
		color: #625f68;
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
	.intro {
		margin-top: 0.25rem;
	}
	.section_title {
		margin-bottom: 0.75rem;
		font-size: 0.875rem;
		font-weight: 600;
		color: #111013;
	}
	.query_error {
		padding: 2rem;
		font-size: 0.875rem;
		font-weight: 500;
		color: #b91c1c;
	}
	.operation_status {
		margin-bottom: 0.75rem;
		font-size: 0.75rem;
	}
	.operation_idle {
		color: #9a96a3;
	}
	.operation_pending {
		color: #625f68;
	}
	.operation_error {
		font-weight: 500;
		color: #b91c1c;
	}
	.operation_divider {
		margin-top: 1rem;
		margin-bottom: 1rem;
		border-top-width: 1px;
		border-top-style: solid;
		border-top-color: #d8d2c3;
	}
};
