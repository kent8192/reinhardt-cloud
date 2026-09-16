//! Generated component styles shared by Dashboard client applications.

use reinhardt::pages::style_def;

/// Typed class tokens for Dashboard's shared client primitives.
#[style_def]
pub static STYLES: SharedStyles = style! {
	.app {
		min-height: 100vh;
		background-color: #f6f5f2;
		font-family: sans-serif;
		color: #111013;
	}
	.shell {
		box-sizing: border-box;
		max-width: 80rem;
		margin: 0;
		margin-left: auto;
		margin-right: auto;
		padding: 1.5rem;
		@media (min-width: 1024px) {
			padding: 2rem;
		}
	}
	.topline {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		margin-bottom: 1.5rem;
		padding-bottom: 1.25rem;
		border-bottom-width: 1px;
		border-bottom-style: solid;
		border-bottom-color: #d8d2c3;
		@media (min-width: 640px) {
			flex-direction: row;
			align-items: flex-end;
			justify-content: space-between;
		}
	}
	.kicker {
		font-size: 0.6875rem;
		font-weight: 700;
		text-transform: uppercase;
		color: #0a4d48;
	}
	.title {
		font-size: 1.7rem;
		font-weight: 700;
		line-height: 1.25;
		color: #111013;
	}
	.muted {
		font-size: 0.875rem;
		color: #625f68;
	}
	.stack {
		display: grid;
		gap: 1rem;
	}
	.panel {
		border-width: 1px;
		border-style: solid;
		border-color: #d8d2c3;
		border-radius: 0.375rem;
		background-color: white;
	}
	.panel_pad {
		padding: 1rem;
		border-width: 1px;
		border-style: solid;
		border-color: #d8d2c3;
		border-radius: 0.375rem;
		background-color: white;
	}
	.panel_head {
		padding: 0.75rem;
		border-bottom-width: 1px;
		border-bottom-style: solid;
		border-bottom-color: #d8d2c3;
		background-color: #e9e6dd;
		font-size: 0.75rem;
		font-weight: 700;
		text-transform: uppercase;
		color: #625f68;
	}
	.form {
		display: grid;
		align-items: start;
		gap: 1rem;
		label {
			display: block;
			margin-bottom: 0.375rem;
			font-size: 0.6875rem;
			font-weight: 700;
			line-height: 1;
			text-transform: uppercase;
			color: #625f68;
		}
	}
	.form_grid {
		display: grid;
		align-items: start;
		gap: 1rem;
		label {
			display: block;
			margin-bottom: 0.375rem;
			font-size: 0.6875rem;
			font-weight: 700;
			line-height: 1;
			text-transform: uppercase;
			color: #625f68;
		}
	}
	.form_stack {
		display: grid;
		align-items: start;
		gap: 1rem;
		label {
			display: block;
			margin-bottom: 0.375rem;
			font-size: 0.6875rem;
			font-weight: 700;
			line-height: 1;
			text-transform: uppercase;
			color: #625f68;
		}
	}
	.field {
		min-width: 0;
	}
	.label {
		display: block;
		margin-bottom: 0.375rem;
		font-size: 0.6875rem;
		font-weight: 700;
		line-height: 1;
		text-transform: uppercase;
		color: #625f68;
	}
	.input {
		box-sizing: border-box;
		width: 100%;
		min-height: 2.75rem;
		padding: 0.75rem;
		border-width: 1px;
		border-style: solid;
		border-color: #d8d2c3;
		border-radius: 0.375rem;
		background-color: white;
		font-size: 0.875rem;
		line-height: 1.25rem;
		color: #111013;
		outline: none;
		transition-duration: 150ms;
		&:hover {
			border-color: #bdb5a5;
		}
		&:focus {
			border-color: #147d74;
			background-color: #fffdfa;
		}
		&:disabled {
			cursor: not-allowed;
			background-color: #e9e6dd;
			color: #9a96a3;
		}
		&[aria-invalid=true] {
			border-color: #dc2626;
		}
	}
	.textarea {
		min-height: 10rem;
		resize: vertical;
		font-family: monospace;
		font-size: 0.75rem;
		line-height: 1.625;
	}
	.checkbox {
		box-sizing: border-box;
		width: 1rem;
		height: 1rem;
		outline: none;
		&:focus {
			outline-color: #147d74;
		}
		&:disabled {
			cursor: not-allowed;
			opacity: 0.6;
		}
	}
	.checkbox_field {
		display: grid;
		align-items: center;
		gap: 0.5rem;
		> input {
			order: -1;
		}
		> label {
			margin-bottom: 0;
		}
	}
	.button_primary {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 0.75rem;
		border: 0;
		border-radius: 0.375rem;
		background-color: #106b64;
		font-size: 0.875rem;
		font-weight: 700;
		color: white;
		cursor: pointer;
		&:hover {
			background-color: #0a4d48;
		}
		&:focus-visible {
			outline-color: #147d74;
		}
		&:disabled {
			cursor: not-allowed;
			opacity: 0.6;
		}
	}
	.button_secondary {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 0.75rem;
		border-width: 1px;
		border-style: solid;
		border-color: #d8d2c3;
		border-radius: 0.375rem;
		background-color: white;
		font-size: 0.875rem;
		font-weight: 700;
		color: #2b2a30;
		cursor: pointer;
		&:hover {
			border-color: #147d74;
			background-color: #f6f5f2;
			color: #111013;
		}
		&:focus-visible {
			outline-color: #147d74;
		}
		&:disabled {
			cursor: not-allowed;
			opacity: 0.6;
		}
	}
	.button_dark {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 0.75rem;
		border: 0;
		border-radius: 0.375rem;
		background-color: #111013;
		font-size: 0.875rem;
		font-weight: 700;
		color: white;
		cursor: pointer;
		&:hover {
			background-color: #2b2a30;
		}
		&:focus-visible {
			outline-color: #9a96a3;
		}
		&:disabled {
			cursor: not-allowed;
			opacity: 0.6;
		}
	}
	.button_warning {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 0.75rem;
		border: 0;
		border-radius: 0.375rem;
		background-color: #9c3f1a;
		font-size: 0.875rem;
		font-weight: 700;
		color: white;
		cursor: pointer;
		&:hover {
			background-color: #c45a23;
		}
		&:focus-visible {
			outline-color: #c45a23;
		}
		&:disabled {
			cursor: not-allowed;
			opacity: 0.6;
		}
	}
	.button_danger {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 0.75rem;
		border: 0;
		border-radius: 0.375rem;
		background-color: #b91c1c;
		font-size: 0.875rem;
		font-weight: 700;
		color: white;
		cursor: pointer;
		&:hover {
			background-color: #991b1b;
		}
		&:focus-visible {
			outline-color: #ef4444;
		}
		&:disabled {
			cursor: not-allowed;
			opacity: 0.6;
		}
	}
	.link {
		display: inline-flex;
		align-items: center;
		padding: 0;
		border: 0;
		background-color: transparent;
		font-size: 0.875rem;
		font-weight: 700;
		color: #0a4d48;
		cursor: pointer;
		text-decoration: none;
		&:hover {
			text-decoration: underline;
		}
	}
	.table {
		min-width: 100%;
		font-size: 0.875rem;
	}
	.table_header {
		padding: 0.75rem;
		text-align: left;
		font-size: 0.6875rem;
		font-weight: 700;
		text-transform: uppercase;
		color: #625f68;
	}
	.table_cell {
		padding: 0.75rem;
		color: #625f68;
	}
	.empty {
		padding: 2rem;
		font-size: 0.875rem;
		color: #625f68;
	}
	.notice {
		padding: 0.75rem;
		border-radius: 0.375rem;
		font-size: 0.875rem;
	}
	.notice_info {
		border-color: #bfdbfe;
		background-color: #eff6ff;
		color: #1e3a8a;
	}
	.notice_warning {
		border-color: #fde68a;
		background-color: #fffbeb;
		color: brown;
	}
	.notice_critical {
		border-color: #fecaca;
		background-color: #fef2f2;
		color: #991b1b;
	}
	.entity_select {
		min-width: 0;
	}
	.entity_select_label {
		display: block;
		margin-bottom: 0.375rem;
		font-size: 0.6875rem;
		font-weight: 700;
		line-height: 1;
		text-transform: uppercase;
		color: #625f68;
	}
	.entity_select_control {
		min-width: 0;
	}
	.status_badge {
		display: inline-flex;
		align-items: center;
		padding: 0.625rem;
		border-radius: 9999px;
		font-size: 0.75rem;
		font-weight: 600;
	}
	.status_running {
		background-color: #dcfce7;
		color: #166534;
	}
	.status_deploying {
		background-color: #dbeafe;
		color: #1e40af;
	}
	.status_degraded {
		background-color: #fef3c7;
		color: brown;
	}
	.status_failed {
		background-color: #fee2e2;
		color: #991b1b;
	}
	.status_stopped {
		background-color: #f3f4f6;
		color: #374151;
	}
	.toast_container {
		position: fixed;
		top: 1rem;
		right: 1rem;
		z-index: 50;
		display: grid;
		gap: 0.5rem;
		max-width: 24rem;
	}
	.toast {
		padding: 1rem;
		border-width: 1px;
		border-style: solid;
		border-color: transparent;
		border-radius: 0.375rem;
	}
	.toast_content {
		display: grid;
		align-items: start;
		gap: 0.75rem;
	}
	.toast_icon {
		flex-shrink: 0;
		font-size: 1.125rem;
	}
	.toast_body {
		min-width: 0;
	}
	.toast_title {
		margin: 0;
		font-size: 0.875rem;
		font-weight: 600;
		color: #111013;
	}
	.toast_message {
		margin-top: 0.125rem;
		font-size: 0.875rem;
		color: #625f68;
	}
	.toast_info {
		border-color: #bfdbfe;
		background-color: #eff6ff;
	}
	.toast_warning {
		border-color: #fde68a;
		background-color: #fffbeb;
	}
	.toast_critical {
		border-color: #fecaca;
		background-color: #fef2f2;
	}
	.not_found_page {
		display: grid;
		min-height: 100vh;
		place-items: center;
		padding: 1rem;
		background-color: #f6f5f2;
	}
	.not_found_content {
		text-align: center;
	}
	.not_found_code {
		margin: 1rem;
		font-size: 3.75rem;
		font-weight: 600;
		color: #d8d2c3;
	}
	.not_found_message {
		margin: 2rem;
		font-size: 1.25rem;
		color: #625f68;
	}
	.not_found_action {
		padding: 1rem;
	}
};
