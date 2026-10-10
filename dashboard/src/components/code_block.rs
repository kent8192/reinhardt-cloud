//! Code block with a copy button.
//!
//! `reinhardt-pages` checked: `ui::ActionButton` dispatches an async `Action`,
//! and the only clipboard support is the `ClipboardEvent` payload in `event`
//! (no write helper). Copying is a browser call with a timed label, so the block
//! and its copy button are custom.

use reinhardt::pages::component::Page;
use reinhardt::pages::event::ClickEvent;
use reinhardt::pages::reactive::Signal;
use reinhardt::pages::{Callback, TranslatedText, page, style_def, t};

use crate::components::browser;
use crate::components::button::{ButtonSize, ButtonVariant, button_classes};

/// How long the copy button reports success before it reverts, in milliseconds.
const COPIED_LABEL_MILLIS: i32 = 1600;

#[style_def]
pub static CODE_BLOCK_STYLES: CodeBlockStyles = style! {
	globals {
		space_1: Length;
		space_3: Length;
		space_4: Length;
		radius_md: Length;
		surface_sunken: Color;
		border_default: Color;
		ink_strong: Color;
		ink_muted: Color;
		text_sm: Length;
	}
	.code {
		background: globals.surface_sunken;
		border: (1px, solid, globals.border_default);
		border-radius: globals.radius_md;
		pre {
			margin: 0;
			padding: (globals.space_3, globals.space_4);
			overflow-x: auto;
			color: globals.ink_strong;
			font-size: globals.text_sm;
			line-height: 1.6;
		}
	}
	.bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: globals.space_3;
		padding: (globals.space_1, globals.space_1, globals.space_1, globals.space_4);
		border-bottom: (1px, solid, globals.border_default);
		color: globals.ink_muted;
		font-size: globals.text_sm;
	}
};

/// Renders `text` in a code block with a bar holding `title` and a copy button.
///
/// `id` must be unique in the document; the copy button controls the `<code>`
/// element with the ID `{id}-text`. The button's accessible name is computed
/// from its content: the visible word ("Copy", then "Copied" after a copy)
/// followed by `title` in a visually hidden span, so several blocks on one page
/// can be told apart and the name always contains the visible label.
pub fn code_block(id: &str, title: TranslatedText, text: String) -> Page {
	let copied = Signal::new(false);
	let copy = Callback::new({
		let text = text.clone();
		move |_event: ClickEvent| {
			browser::copy_text(&text, move || {
				copied.set(true);
				browser::after_millis(COPIED_LABEL_MILLIS, move || copied.set(false));
			});
		}
	});
	let button_class = button_classes(ButtonVariant::Quiet, ButtonSize::Small);
	let text_id = format!("{id}-text");
	let controls = text_id.clone();
	page!({
		div {
			class: CODE_BLOCK_STYLES.code(),
			div {
				class: CODE_BLOCK_STYLES.bar(),
				span { { title } }
				button {
					class: button_class,
					type: "button",
					aria_controls: controls,
					@click: copy,
					if copied.get() { { t!("Copied") } }
					else { { t!("Copy") } }span {
						class: "rc-visually-hidden",
						" " { title.clone() }
					}
				}
			}
			pre {
				class: "rc-font-mono",
				code {
					id: text_id,
					{ text }
				}
			}
		}
	})
}
