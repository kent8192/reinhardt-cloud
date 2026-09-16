//! Generated component styles owned by the authentication client.

use reinhardt::pages::style_def;

/// Typed class tokens for authentication and account views.
#[style_def]
pub static STYLES: AuthStyles = style! {
	.auth_page {
		display: grid;
		min-height: 100vh;
		place-items: center;
		padding: 1rem;
	}
	.auth_card {
		width: 100%;
		max-width: 28rem;
	}
	.auth_brand {
		margin-bottom: 2rem;
		text-align: center;
	}
	.auth_kicker {
		margin-bottom: 0.5rem;
	}
	.auth_brand_name {
		font-size: 1.875rem;
		font-weight: 600;
		color: #111013;
	}
	.auth_brand_subtitle {
		margin-top: 0.25rem;
	}
	.auth_panel {
		padding: 2rem;
	}
	.auth_form_title {
		margin-bottom: 1.5rem;
		text-align: center;
		font-size: 1.25rem;
		font-weight: 600;
		color: #111013;
	}
	.field_error {
		margin-top: 0.25rem;
		font-size: 0.75rem;
		font-weight: 500;
		color: #b91c1c;
	}
	.form_error {
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
	.form_submit {
		width: 100%;
		min-height: 2.75rem;
		font-size: 1rem;
	}
	.auth_footer {
		margin-top: 1.5rem;
		text-align: center;
		font-size: 0.875rem;
		color: #625f68;
	}
	.oauth_section {
		display: grid;
		gap: 1rem;
		margin-top: 1.5rem;
	}
	.oauth_divider {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-size: 0.875rem;
		color: #625f68;
	}
	.oauth_divider_line {
		flex: 1;
		border-top-width: 1px;
		border-top-style: solid;
		border-top-color: #d8d2c3;
	}
	.oauth_options {
		display: grid;
		gap: 0.5rem;
	}
	.oauth_button {
		width: 100%;
	}
	.oauth_status {
		margin-top: 1rem;
		text-align: center;
		font-size: 0.75rem;
		font-weight: 500;
		color: #625f68;
	}
	.oauth_error {
		color: #b91c1c;
	}
	.oauth_warning {
		color: brown;
	}
	.account_grid {
		display: grid;
		gap: 1rem;
		@media (min-width: 1024px) {
			grid-template-columns: unchecked_fn!(repeat(2, minmax(0, 1fr)));
		}
	}
	.account_heading {
		font-size: 1rem;
		font-weight: 600;
		color: #111013;
	}
	.account_details {
		display: grid;
		gap: 0.75rem;
		margin-top: 1rem;
		font-size: 0.875rem;
	}
	.account_term {
		font-weight: 500;
		color: #625f68;
	}
	.account_value {
		margin-top: 0.25rem;
		color: #111013;
	}
	.account_provider_head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 1rem;
	}
	.account_badge {
		display: inline-flex;
		flex-shrink: 0;
		padding: 0.25rem;
		padding-left: 0.625rem;
		padding-right: 0.625rem;
		border-radius: 9999px;
		background-color: #dbeafe;
		font-size: 0.75rem;
		font-weight: 600;
		color: #0a4d48;
	}
	.account_action_spacing {
		margin-top: 1.25rem;
	}
	.account_status {
		font-size: 0.875rem;
		font-weight: 500;
		color: #2b2a30;
	}
	.account_refresh_notice {
		padding: 0.5rem;
		padding-left: 1rem;
		padding-right: 1rem;
		border-bottom-width: 1px;
		border-bottom-style: solid;
		font-size: 0.75rem;
		font-weight: 500;
	}
	.account_refresh_warning {
		border-bottom-color: #fde68a;
		background-color: #fffbeb;
		color: brown;
	}
	.account_refresh_info {
		border-bottom-color: #d8d2c3;
		background-color: #f6f5f2;
		color: #625f68;
	}
};
