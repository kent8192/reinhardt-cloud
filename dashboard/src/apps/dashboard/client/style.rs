//! Generated component styles owned by the dashboard shell.

use reinhardt::pages::style_def;

/// Typed class tokens for dashboard navigation and overview views.
#[style_def]
pub static STYLES: DashboardStyles = style! {
	.dashboard_app {
		display: flex;
		flex-direction: column;
	}
	.dashboard_header {
		position: sticky;
		top: 0;
		z-index: 10;
		display: flex;
		height: 4rem;
		align-items: center;
		justify-content: space-between;
		padding-left: 1rem;
		padding-right: 1rem;
		border-bottom-width: 1px;
		border-bottom-style: solid;
		border-bottom-color: #d8d2c3;
		background-color: #fffdfa;
		@media (min-width: 640px) {
			padding-left: 1.5rem;
			padding-right: 1.5rem;
		}
	}
	.header_brand {
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}
	.brand_mark {
		display: grid;
		width: 2.25rem;
		height: 2.25rem;
		place-items: center;
		border-radius: 0.375rem;
		background-color: #111013;
		font-size: 0.875rem;
		font-weight: 700;
		color: white;
	}
	.brand_name {
		display: block;
		font-size: 1rem;
		font-weight: 700;
		line-height: 1.25;
		color: #111013;
	}
	.brand_subtitle {
		display: none;
		font-size: 0.75rem;
		font-weight: 600;
		text-transform: uppercase;
		color: #625f68;
		@media (min-width: 640px) {
			display: block;
		}
	}
	.header_actions {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		@media (min-width: 640px) {
			gap: 0.75rem;
		}
	}
	.header_action {
		font-size: 0.875rem;
	}
	.dashboard_body {
		display: flex;
		flex: 1;
		flex-direction: column;
		@media (min-width: 768px) {
			flex-direction: row;
		}
	}
	.sidebar {
		box-sizing: border-box;
		width: 100%;
		padding: 1rem;
		border-bottom-width: 1px;
		border-bottom-style: solid;
		border-bottom-color: #d8d2c3;
		background-color: #f6f5f2;
		@media (min-width: 768px) {
			width: 16rem;
			border-right-width: 1px;
			border-right-style: solid;
			border-right-color: #d8d2c3;
			border-bottom: 0;
			background-color: #fffdfa;
		}
	}
	.organization {
		margin-bottom: 1rem;
		padding: 0.75rem;
		border-width: 1px;
		border-style: solid;
		border-color: #d8d2c3;
		border-radius: 0.375rem;
		background-color: white;
	}
	.organization_label {
		font-size: 0.75rem;
		font-weight: 700;
		text-transform: uppercase;
		color: #625f68;
	}
	.organization_name {
		margin-top: 0.25rem;
		overflow: hidden;
		font-size: 0.875rem;
		font-weight: 700;
		color: #111013;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.navigation_list {
		display: grid;
		gap: 0.375rem;
	}
	.navigation_item {
		display: block;
		padding: 0.5rem;
		padding-left: 0.75rem;
		padding-right: 0.75rem;
		border-width: 1px;
		border-style: solid;
		border-color: transparent;
		border-radius: 0.375rem;
		font-size: 0.875rem;
		font-weight: 600;
		color: #625f68;
		text-decoration: none;
		&:hover {
			border-color: #d8d2c3;
			background-color: white;
			color: #111013;
		}
		&:focus-visible {
			outline-color: #147d74;
		}
	}
	.navigation_item_active {
		display: block;
		padding: 0.5rem;
		padding-left: 0.75rem;
		padding-right: 0.75rem;
		border-width: 1px;
		border-style: solid;
		border-color: #a6ddd8;
		border-radius: 0.375rem;
		background-color: #e0f2f0;
		font-size: 0.875rem;
		font-weight: 700;
		color: #0a4d48;
		text-decoration: none;
		&:focus-visible {
			outline-color: #147d74;
		}
	}
	.dashboard_main {
		min-width: 0;
		flex: 1;
	}
	.overview_title {
		margin-top: 0.25rem;
	}
	.overview_description {
		max-width: 36rem;
	}
	.overview_metrics {
		display: grid;
		gap: 1rem;
		@media (min-width: 768px) {
			grid-template-columns: unchecked_fn!(repeat(2, minmax(0, 1fr)));
		}
	}
	.metric_card {
		border-left-width: 0.25rem;
		border-left-style: solid;
	}
	.metric_clusters {
		border-left-color: #147d74;
	}
	.metric_deployments {
		border-left-color: #1d4ed8;
	}
	.metric_label {
		font-size: 0.75rem;
		font-weight: 700;
		text-transform: uppercase;
		color: #625f68;
	}
	.metric_value {
		margin-top: 0.75rem;
		font-size: 1.875rem;
		font-weight: 700;
		color: #111013;
	}
	.metric_detail {
		margin-top: 0.25rem;
		font-size: 0.75rem;
		font-weight: 600;
		color: #625f68;
	}
	.overview_panels {
		display: grid;
		gap: 1rem;
		margin-top: 1.5rem;
		@media (min-width: 1024px) {
			grid-template-columns: (1.2fr, 0.8fr);
		}
	}
	.runbook_list {
		display: grid;
	}
	.runbook_link {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0.75rem;
		padding-left: 1rem;
		padding-right: 1rem;
		border-bottom-width: 1px;
		border-bottom-style: solid;
		border-bottom-color: #d8d2c3;
		font-size: 0.875rem;
		font-weight: 600;
		color: #2b2a30;
		text-decoration: none;
		&:last-child {
			border-bottom: 0;
		}
		&:hover {
			background-color: #f6f5f2;
		}
		&:focus-visible {
			outline-color: #147d74;
		}
	}
	.runbook_link_action {
		color: #0a4d48;
	}
	.control_surface {
		background-color: #111013;
		color: white;
	}
	.control_surface_label {
		font-size: 0.75rem;
		font-weight: 700;
		text-transform: uppercase;
		color: #d8d2c3;
	}
	.control_surface_title {
		margin-top: 0.75rem;
		font-size: 1.5rem;
		font-weight: 700;
	}
	.control_surface_description {
		margin-top: 0.5rem;
		font-size: 0.875rem;
		color: #e9e6dd;
	}
};
