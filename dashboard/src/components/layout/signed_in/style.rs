//! Styles of the signed-in layout: the sidebar, its navigation and menus, and
//! the page heading.
//!
//! Colors, spacing, radii, and type sizes come from the tokens in
//! `static/css/tokens.css`; what the `style!` DSL cannot express is composed
//! from the `rc-` utilities in `static/css/utilities.css`.

use reinhardt::pages::style_def;

#[style_def]
pub static SIGNED_IN_STYLES: SignedInStyles = style! {
	globals {
		space_1: Length;
		space_2: Length;
		space_3: Length;
		space_4: Length;
		space_5: Length;
		space_6: Length;
		space_7: Length;
		space_8: Length;
		radius_md: Length;
		radius_lg: Length;
		border_width: Length;
		border_subtle: Color;
		border_default: Color;
		surface_raised: Color;
		surface_hover: Color;
		surface_inverse: Color;
		ink_inverse: Color;
		ink_strong: Color;
		ink_default: Color;
		ink_muted: Color;
		ink_subtle: Color;
		text_xs: Length;
		weight_medium: Number;
		weight_semibold: Number;
		weight_regular: Number;
		leading_tight: Number;
		leading_prose: Number;
		tracking_display: Length;
		tracking_heading: Length;
		text_lg: Length;
		text_2xl: Length;
		measure_prose: Length;
		sidebar_width: Length;
		sidebar_brand_mark_width: Length;
		measure_page: Length;
		control_height_lg: Length;
		skip_link_hidden_top: Length;
	}
	.app {
		display: grid;
		grid-template-columns: (globals.sidebar_width, 1fr);
		min-height: 100vh;
	}
	.skip {
		position: absolute;
		left: globals.space_3;
		top: globals.skip_link_hidden_top;
		z-index: 100;
		padding: (globals.space_2, globals.space_3);
		background: globals.surface_inverse;
		color: globals.ink_inverse;
		border-radius: globals.radius_md;
		&:focus {
			top: globals.space_3;
		}
	}
	.sidebar {
		position: sticky;
		top: 0;
		align-self: start;
		display: flex;
		flex-direction: column;
		gap: globals.space_5;
		height: 100vh;
		padding: (globals.space_4, globals.space_3);
		border-right: (globals.border_width, solid, globals.border_subtle);
	}
	.brand {
		display: flex;
		align-items: center;
		gap: globals.space_2;
		padding: (globals.space_1, globals.space_2);
		color: globals.ink_strong;
		font-weight: globals.weight_semibold;
		text-decoration: none;
		letter-spacing: globals.tracking_display;
		img {
			width: globals.sidebar_brand_mark_width;
			height: auto;
		}
	}
	.nav {
		display: flex;
		flex-direction: column;
		gap: globals.space_1;
	}
	.nav_link {
		display: flex;
		gap: globals.space_2;
		align-items: center;
		justify-content: space-between;
		padding: (globals.space_2, globals.space_3);
		border-radius: globals.radius_md;
		color: globals.ink_muted;
		text-decoration: none;
		font-weight: globals.weight_medium;
		&:hover {
			background: globals.surface_hover;
			color: globals.ink_strong;
		}
	}
	.nav_current {
		background: globals.surface_raised;
		color: globals.ink_strong;
	}
	.foot {
		margin-top: auto;
		display: flex;
		flex-direction: column;
		gap: globals.space_2;
	}
	.menu {
		position: relative;
	}
	.toggle {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: globals.space_2;
		width: 100%;
		min-height: globals.control_height_lg;
		padding: (globals.space_2, globals.space_3);
		background: globals.surface_raised;
		border: (globals.border_width, solid, globals.border_default);
		border-radius: globals.radius_md;
		color: globals.ink_strong;
		text-align: left;
	}
	.panel {
		position: absolute;
		z-index: 20;
		left: 0;
		right: 0;
		bottom: 100% + globals.space_1;
		display: flex;
		flex-direction: column;
		padding: globals.space_1;
		background: globals.surface_raised;
		border: (globals.border_width, solid, globals.border_default);
		border-radius: globals.radius_lg;
	}
	.item {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: globals.space_2;
		width: 100%;
		min-height: globals.control_height_lg;
		padding: (globals.space_2, globals.space_3);
		background: transparent;
		border: 0;
		border-radius: globals.radius_md;
		color: globals.ink_default;
		text-align: left;
		text-decoration: none;
		cursor: pointer;
		&:hover {
			background: globals.surface_hover;
		}
	}
	.name {
		display: block;
		font-weight: globals.weight_semibold;
		line-height: globals.leading_tight;
	}
	.sub {
		display: block;
		color: globals.ink_subtle;
		font-size: globals.text_xs;
	}
	.person {
		display: flex;
		align-items: center;
		gap: globals.space_3;
	}
	.theme {
		button {
			width: 100%;
			justify-content: flex-start;
			color: globals.ink_muted;
		}
	}
	.main {
		min-width: 0;
		padding: (globals.space_6, globals.space_7, globals.space_8);
	}
	.page {
		max-width: globals.measure_page;
		display: flex;
		flex-direction: column;
		gap: globals.space_6;
	}
	.page_head {
		display: flex;
		flex-direction: column;
		gap: globals.space_2;
	}
	.page_title {
		font-size: globals.text_2xl;
		font-weight: globals.weight_regular;
		letter-spacing: globals.tracking_display;
		line-height: globals.leading_tight;
		color: globals.ink_strong;
	}
	.page_lede {
		max-width: globals.measure_prose;
		color: globals.ink_muted;
		line-height: globals.leading_prose;
	}
	.section_title {
		margin-bottom: globals.space_3;
		font-size: globals.text_lg;
		font-weight: globals.weight_semibold;
		letter-spacing: globals.tracking_heading;
		color: globals.ink_strong;
	}
	@media (max-width: 62rem) {
		.app {
			grid-template-columns: unchecked_fn!(minmax(0, 1fr));
		}
		.sidebar {
			position: static;
			height: auto;
			gap: globals.space_3;
			border-right: 0;
			border-bottom: (globals.border_width, solid, globals.border_subtle);
		}
		.nav {
			flex-direction: row;
			flex-wrap: wrap;
		}
		.foot {
			flex-direction: row;
			align-items: center;
			margin-top: 0;
		}
		.panel {
			top: 100% + globals.space_1;
			bottom: auto;
		}
		.main {
			padding: (globals.space_5, globals.space_4, globals.space_8);
		}
	}
};
