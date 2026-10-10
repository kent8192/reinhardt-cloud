//! Design styles for data tables.
//!
//! `reinhardt-pages` checked: `tables::Table` and `tables::ColumnTrait` define
//! columns, sorting, pagination, and cell rendering, but they build the table
//! as a web-sys `dom::Element` (`Table::render`, `Column::render`) that exists
//! only in the browser, and the provided columns expose no class hook beyond
//! `header_attrs` / `cell_attrs`. There is no `page!` table to style, so no
//! table component lives here. An implementation of `Table` applies these
//! scoped classes in its `render` (`wrap` around the table, `table` on the
//! `<table>`, `end` on right-aligned cells through `cell_attrs`).

use reinhardt::pages::style_def;

#[style_def]
pub static TABLE_STYLES: TableStyles = style! {
	globals {
		space_3: Length;
		space_4: Length;
		radius_lg: Length;
		surface_raised: Color;
		surface_page: Color;
		surface_hover: Color;
		border_subtle: Color;
		border_default: Color;
		ink_muted: Color;
		text_sm: Length;
		weight_regular: Number;
		weight_medium: Number;
	}
	.wrap {
		overflow-x: auto;
		background: globals.surface_raised;
		border: (1px, solid, globals.border_default);
		border-radius: globals.radius_lg;
	}
	.table {
		width: 100%;
		th, td {
			padding: (globals.space_3, globals.space_4);
			text-align: left;
			border-bottom: (1px, solid, globals.border_subtle);
		}
		thead {
			th {
				color: globals.ink_muted;
				font-size: globals.text_sm;
				font-weight: globals.weight_medium;
				white-space: nowrap;
				background: globals.surface_page;
				border-bottom-color: globals.border_default;
			}
		}
		tbody {
			th {
				font-weight: globals.weight_regular;
			}
			tr {
				&:last-child {
					td, th {
						border-bottom: 0;
					}
				}
				&:hover {
					background: globals.surface_hover;
				}
			}
		}
		/* Nested so it outranks the `th, td` alignment above. */
		.end {
			text-align: right;
		}
	}
};
