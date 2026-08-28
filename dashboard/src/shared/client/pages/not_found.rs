//! 404 Not Found page.

use reinhardt::pages::component::Page;
use reinhardt::pages::page;

use crate::shared::client::routes::route_href;
use crate::shared::client::style::STYLES;

/// Render a centered 404 page with a link back to the dashboard.
pub fn not_found_page() -> Page {
	let home_href = route_href("dashboard:home", "/");
	page!({
		div {
			class: STYLES.not_found_page(),
			div {
				class: STYLES.not_found_content(),
				h1 {
					class: STYLES.not_found_code(),
					"404"
				}
				p {
					class: STYLES.not_found_message(),
					"Page not found"
				}
				a {
					href: home_href,
					class: STYLES.button_primary() + STYLES.not_found_action(),
					"Back to Dashboard"
				}
			}
		}
	})
}
