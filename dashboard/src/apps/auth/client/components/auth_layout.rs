//! Centered authentication layout wrapper.
//!
//! Provides a full-height centered card layout with Reinhardt Cloud branding,
//! used as a shared shell for login and register pages.

use reinhardt::pages::component::Page;
use reinhardt::pages::page;

use crate::apps::auth::client::style::STYLES;
use crate::shared::client::style::STYLES as SHARED_STYLES;

/// Render a centered authentication layout with a branded card.
///
/// The `title` is shown below the Reinhardt Cloud header and `form_content`
/// is the page-specific form view rendered inside the card body.
pub fn auth_layout(title: &str, form_content: Page) -> Page {
	let title = title.to_string();
	page!({
		div {
			class: SHARED_STYLES.app() + STYLES.auth_page(),
			div {
				class: STYLES.auth_card(),
				div {
					class: STYLES.auth_brand(),
					p {
						class: SHARED_STYLES.kicker() + STYLES.auth_kicker(),
						"Control plane"
					}
					h1 {
						class: STYLES.auth_brand_name(),
						"Reinhardt Cloud"
					}
					p {
						class: SHARED_STYLES.muted() + STYLES.auth_brand_subtitle(),
						"Cloud Platform"
					}
				}
				div {
					class: SHARED_STYLES.panel_pad() + STYLES.auth_panel(),
					h2 {
						class: STYLES.auth_form_title(),
						{ title }
					}
					{ form_content }
				}
			}
		}
	})
}
