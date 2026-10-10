//! Component styles for the accounts application.
//!
//! The sign-in page and its decorative product preview (Variant A of the
//! approved design). Colors, spacing, radii, and type sizes come from the
//! tokens; what the `style!` DSL cannot express (font stacks, elevations, the
//! progress wash) is composed from the `rc-` utilities in
//! `static/css/utilities.css`.

use reinhardt::pages::style_def;

/// Styles of the sign-in page.
#[style_def]
pub static STYLES: AccountsStyles = style! {
	globals {
		space_1: Length;
		space_2: Length;
		space_3: Length;
		space_4: Length;
		space_5: Length;
		radius_sm: Length;
		radius_lg: Length;
		radius_full: Length;
		border_width: Length;
		preview_phase_rule: Length;
		text_sm: Length;
		text_xl: Length;
		text_3xl: Length;
		weight_regular: Number;
		weight_medium: Number;
		weight_semibold: Number;
		leading_prose: Number;
		leading_display: Number;
		leading_log: Number;
		tracking_hero: Length;
		tracking_heading: Length;
		measure_prose: Length;
		surface_raised: Color;
		border_default: Color;
		border_subtle: Color;
		ink_strong: Color;
		ink_muted: Color;
		ink_subtle: Color;
		status_success_fg: Color;
		status_progress_fg: Color;
		log_bg: Color;
		log_ink: Color;
		log_muted: Color;
		log_info: Color;
		log_warn: Color;
		log_border: Color;
		gold_500: Color;
		preview_deploy_top: Length;
		preview_deploy_end: Length;
		preview_deploy_width: Length;
		preview_log_end: Length;
		preview_log_bottom: Length;
		preview_log_width: Length;
		preview_log_height: Length;
		preview_log_time_width: Length;
		preview_log_level_width: Length;
		preview_live_marker: Length;
		preview_project_chip_top: Length;
		preview_project_chip_end: Length;
		preview_cluster_chip_top: Length;
		preview_cluster_chip_end: Length;
	}
	.title {
		font-size: globals.text_3xl;
		font-weight: globals.weight_regular;
		line-height: globals.leading_display;
		letter-spacing: globals.tracking_hero;
		color: globals.ink_strong;
	}
	.lede {
		max-width: globals.measure_prose;
		line-height: globals.leading_prose;
	}
	.foot {
		color: globals.ink_subtle;
		font-size: globals.text_sm;
	}
	.card {
		position: absolute;
		background: globals.surface_raised;
		border: (globals.border_width, solid, globals.border_default);
		border-radius: globals.radius_lg;
	}
	.deploy {
		top: 50% - globals.preview_deploy_top;
		right: globals.preview_deploy_end;
		width: globals.preview_deploy_width;
		padding: globals.space_5;
	}
	.head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: globals.space_4;
		margin-bottom: globals.space_5;
	}
	.card_title {
		color: globals.ink_strong;
		font-size: globals.text_xl;
		letter-spacing: globals.tracking_heading;
	}
	.card_sub {
		color: globals.ink_muted;
		font-size: globals.text_sm;
	}
	.phases {
		display: grid;
		grid-template-columns: unchecked_fn!(repeat(4, minmax(0, 1fr)));
		gap: globals.space_1;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.phase {
		padding-top: globals.space_3;
		border-top-width: globals.preview_phase_rule;
		border-top-style: solid;
	}
	.phase_pending {
		border-top-color: globals.border_default;
		color: globals.ink_subtle;
	}
	.phase_done {
		border-top-color: globals.status_success_fg;
		color: globals.ink_muted;
	}
	.phase_current {
		border-top-color: globals.status_progress_fg;
		color: globals.ink_strong;
		border-radius: (0, 0, globals.radius_sm, globals.radius_sm);
		padding-left: globals.space_2;
		padding-bottom: globals.space_2;
	}
	.phase_name {
		display: block;
		font-weight: globals.weight_semibold;
	}
	.phase_time {
		display: block;
		font-size: globals.text_sm;
	}
	.facts {
		display: flex;
		flex-wrap: wrap;
		gap: (globals.space_2, globals.space_5);
		margin-top: globals.space_5;
		padding-top: globals.space_4;
		border-top: (globals.border_width, solid, globals.border_subtle);
		color: globals.ink_muted;
		font-size: globals.text_sm;
	}
	.fact_value {
		color: globals.ink_strong;
		font-weight: globals.weight_medium;
	}
	.log {
		right: globals.preview_log_end;
		bottom: globals.preview_log_bottom;
		width: globals.preview_log_width;
		height: globals.preview_log_height;
		overflow: hidden;
		background: globals.log_bg;
		color: globals.log_ink;
	}
	.log_bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: globals.space_3;
		padding: (globals.space_2, globals.space_3);
		border-bottom: (globals.border_width, solid, globals.log_border);
		font-size: globals.text_sm;
		font-weight: globals.weight_medium;
	}
	.log_live {
		display: inline-flex;
		align-items: center;
		gap: globals.space_2;
		&::before {
			content: "";
			width: globals.preview_live_marker;
			height: globals.preview_live_marker;
			background: globals.gold_500;
			border-radius: globals.radius_full;
		}
	}
	.log_target {
		color: globals.log_muted;
	}
	.log_body {
		margin: 0;
		padding: (globals.space_3, 0, globals.space_4);
		overflow: hidden;
		font-size: globals.text_sm;
		line-height: globals.leading_log;
		list-style: none;
	}
	.log_line {
		display: grid;
		grid-template-columns: (globals.preview_log_time_width, globals.preview_log_level_width, 1fr);
		gap: globals.space_3;
		padding: (0, globals.space_4);
	}
	.log_time {
		color: globals.log_muted;
	}
	.log_level {
		color: globals.log_muted;
	}
	.level_info {
		color: globals.log_info;
	}
	.level_warn {
		color: globals.log_warn;
	}
	.chip {
		position: absolute;
		display: inline-flex;
		align-items: center;
		gap: globals.space_3;
		padding: (globals.space_2, globals.space_4);
		background: globals.surface_raised;
		border: (globals.border_width, solid, globals.border_default);
		border-radius: globals.radius_full;
		color: globals.ink_strong;
		font-weight: globals.weight_medium;
	}
	.chip_project {
		top: 50% - globals.preview_project_chip_top;
		right: globals.preview_project_chip_end;
	}
	.chip_cluster {
		top: 50% + globals.preview_cluster_chip_top;
		right: globals.preview_cluster_chip_end;
	}
};
