//! Data table with a visually hidden caption.

use std::borrow::Cow;

use reinhardt::pages::component::Page;
use reinhardt::pages::{TranslatedText, page, style_def};

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

/// A column header of a [`data_table`].
#[derive(Clone)]
pub struct TableColumn {
	label: TranslatedText,
	align_end: bool,
}

impl TableColumn {
	/// Creates a left-aligned column.
	pub fn new(label: TranslatedText) -> Self {
		Self {
			label,
			align_end: false,
		}
	}

	/// Right-aligns the column, for numbers and row actions.
	pub fn align_end(mut self) -> Self {
		self.align_end = true;
		self
	}
}

/// Class of a header or body cell; an empty value adds no styling.
fn cell_classes(align_end: bool) -> Cow<'static, str> {
	if align_end {
		TABLE_STYLES.end().into()
	} else {
		Cow::Borrowed("")
	}
}

fn header_cell(column: &TableColumn) -> Page {
	let classes = cell_classes(column.align_end);
	let label = column.label.clone();
	page!({
		th {
			scope: "col",
			class: classes,
			{ label }
		}
	})
}

fn body_row(columns: &[TableColumn], cells: Vec<Page>) -> Page {
	let cells: Vec<Page> = cells
		.into_iter()
		.enumerate()
		.map(|(index, cell)| {
			let classes = cell_classes(columns.get(index).is_some_and(|column| column.align_end));
			if index == 0 {
				page!({
					th {
						scope: "row",
						class: classes,
						{ cell }
					}
				})
			} else {
				page!({
					td {
						class: classes,
						{ cell }
					}
				})
			}
		})
		.collect();
	page!({
		tr {
			for cell in cells { { cell } }
		}
	})
}

/// Renders a table whose first cell of each row is the row header.
///
/// `caption` names the table for assistive technology and is not shown.
pub fn data_table(
	caption: TranslatedText,
	columns: Vec<TableColumn>,
	rows: Vec<Vec<Page>>,
) -> Page {
	let headers: Vec<Page> = columns.iter().map(header_cell).collect();
	let body: Vec<Page> = rows
		.into_iter()
		.map(|cells| body_row(&columns, cells))
		.collect();
	page!({
		div {
			class: TABLE_STYLES.wrap(),
			table {
				class: TABLE_STYLES.table(),
				caption {
					class: "rc-visually-hidden",
					{ caption }
				}
				thead {
					tr {
						for header in headers { { header } }
					}
				}
				tbody {
					for row in body { { row } }
				}
			}
		}
	})
}
