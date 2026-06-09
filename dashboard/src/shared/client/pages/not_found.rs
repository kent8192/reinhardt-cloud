//! 404 Not Found page.

use reinhardt::pages::component::Page;
use reinhardt::pages::page;

use crate::shared::client::routes::route_href;

/// Render a centered 404 page with a link back to the dashboard.
pub fn not_found_page() -> Page {
	let home_href = route_href("dashboard:home", "/");
	page!(|home_href: String| {
		div {
			class: "rc-app flex items-center justify-center px-4",
			div {
				class: "text-center",
				h1 {
					class: "mb-4 text-6xl font-semibold text-cloud-200",
					"404"
				}
				p {
					class: "mb-8 text-xl text-ink-600",
					"Page not found"
				}
				a {
					href: home_href,
					class: "btn-primary px-6 py-3",
					"Back to Dashboard"
				}
			}
		}
	})(home_href)
}
