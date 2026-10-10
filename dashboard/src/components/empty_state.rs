//! Empty state: says what appears here and what to do next.
//!
//! Presentation for the `empty` slot of `ui::ResourcePanel`, which renders only
//! the slot content it is given.

use reinhardt::pages::component::Page;
use reinhardt::pages::{TranslatedText, page, style_def};

#[style_def]
pub static EMPTY_STATE_STYLES: EmptyStateStyles = style! {
	globals {
		space_3: Length;
		space_6: Length;
		border_strong: Color;
		radius_lg: Length;
		ink_strong: Color;
		ink_muted: Color;
		text_lg: Length;
		leading_prose: Number;
		weight_semibold: Number;
		border_width: Length;
		measure_empty: Length;
	}
	.empty {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: globals.space_3;
		padding: globals.space_6;
		border: (globals.border_width, dashed, globals.border_strong);
		border-radius: globals.radius_lg;
		p {
			max-width: globals.measure_empty;
			color: globals.ink_muted;
			line-height: globals.leading_prose;
		}
	}
	.title {
		color: globals.ink_strong;
		font-size: globals.text_lg;
		font-weight: globals.weight_semibold;
	}
};

/// Renders an empty state with a title, an explanation, and an optional action.
pub fn empty_state(title: TranslatedText, body: TranslatedText, action: Option<Page>) -> Page {
	let action = action.unwrap_or_else(Page::empty);
	page!({
		section {
			class: EMPTY_STATE_STYLES.empty(),
			h2 {
				class: EMPTY_STATE_STYLES.title(),
				{ title }
			}
			p { { body } }
			{ action }
		}
	})
}
