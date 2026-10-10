//! Layout of pages shown before sign-in.
//!
//! A brand line with the logo mark and the theme toggle sit above a single
//! main column. An optional decorative aside fills the remaining width on wide
//! screens and is hidden from assistive technology and on small screens.

use reinhardt::pages::component::Page;
use reinhardt::pages::static_resolver::resolve_static;
use reinhardt::pages::{page, style_def, t};

use crate::ui::theme::theme_toggle;

/// Logical path of the small logo mark under `static/`.
pub const LOGO_MARK_PATH: &str = "img/logo-mark-small.png";

/// Pixel size of [`LOGO_MARK_PATH`]; the image is never upscaled.
const LOGO_MARK_SIZE: (u32, u32) = (142, 98);

/// `id` of the main landmark, targeted by the skip link.
pub const MAIN_ID: &str = "main";

#[style_def]
pub static SIGNED_OUT_STYLES: SignedOutStyles = style! {
	globals {
		space_3: Length;
		space_5: Length;
		space_6: Length;
		space_7: Length;
		space_9: Length;
		radius_md: Length;
		surface_inverse: Color;
		ink_inverse: Color;
		ink_strong: Color;
		text_lg: Length;
		weight_semibold: Number;
	}
	.page {
		position: relative;
		display: grid;
		grid-template-columns: (36rem, 1fr);
		align-items: center;
		min-height: 100vh;
		overflow: hidden;
	}
	.skip {
		position: absolute;
		left: globals.space_3;
		top: -4rem;
		z-index: 100;
		padding: (globals.space_3, globals.space_3);
		background: globals.surface_inverse;
		color: globals.ink_inverse;
		border-radius: globals.radius_md;
		&:focus {
			top: globals.space_3;
		}
	}
	.chrome {
		position: absolute;
		z-index: 2;
		top: globals.space_6;
		left: globals.space_7;
		right: globals.space_7;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: globals.space_3;
	}
	.brand {
		display: flex;
		align-items: center;
		gap: globals.space_3;
		color: globals.ink_strong;
		font-size: globals.text_lg;
		font-weight: globals.weight_semibold;
		letter-spacing: -0.2px;
		img {
			width: 2.75rem;
			height: auto;
		}
	}
	.main {
		position: relative;
		z-index: 1;
		display: flex;
		flex-direction: column;
		gap: globals.space_5;
		padding: (globals.space_9, globals.space_7);
	}
	.aside {
		position: relative;
		align-self: stretch;
		min-height: 100vh;
		overflow: hidden;
		pointer-events: none;
		user-select: none;
	}
	@media (max-width: 62rem) {
		.page {
			grid-template-columns: 1fr;
		}
		.chrome {
			top: globals.space_5;
			left: globals.space_5;
			right: globals.space_5;
		}
		.main {
			padding: (globals.space_9, globals.space_5, globals.space_7);
		}
		.aside {
			display: none;
		}
	}
};

/// Wraps `content` in the signed-out layout.
///
/// `content` becomes the contents of the `<main id="main">` landmark; `aside`
/// is decorative and must not carry information or controls.
pub fn signed_out_layout(content: Page, aside: Option<Page>) -> Page {
	let logo = resolve_static(LOGO_MARK_PATH);
	let (logo_width, logo_height) = (LOGO_MARK_SIZE.0.to_string(), LOGO_MARK_SIZE.1.to_string());
	let skip_target = format!("#{MAIN_ID}");
	let inert = true;
	let aside = match aside {
		Some(content) => page!({
			div {
				class: SIGNED_OUT_STYLES.aside(),
				aria_hidden: "true",
				inert: inert,
				{ content }
			}
		}),
		None => Page::empty(),
	};
	page!({
		div {
			class: SIGNED_OUT_STYLES.page(),
			a {
				class: SIGNED_OUT_STYLES.skip(),
				href: skip_target,
				{ t!("Skip to main content") }
			}
			header {
				class: SIGNED_OUT_STYLES.chrome(),
				p {
					class: SIGNED_OUT_STYLES.brand(),
					img {
						src: logo,
						alt: "",
						width: logo_width,
						height: logo_height,
					}
					{ t!("Reinhardt Cloud") }
				}
				{ theme_toggle() }
			}
			main {
				id: MAIN_ID,
				class: SIGNED_OUT_STYLES.main(),
				{ content }
			}
			{ aside }
		}
	})
}
