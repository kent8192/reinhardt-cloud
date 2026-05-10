//! 404 Not Found page.

use reinhardt::pages::component::Page;
use reinhardt::pages::page;

use crate::config::urls::ResolvedUrls;

/// Render a centered 404 page with a link back to the dashboard.
pub fn not_found_page() -> Page {
	page!(|| {
		div {
			class: "min-h-screen flex items-center justify-center bg-gray-50",
			div {
				class: "text-center",
				h1 {
					class: "text-6xl font-bold text-gray-300 mb-4",
					"404"
				}
				p {
					class: "text-xl text-gray-600 mb-8",
					"Page not found"
				}
				a {
					href: ResolvedUrls::from_global().client().dashboard().home(),
					class: "px-6 py-3 bg-blue-600 text-white rounded-lg hover:bg-blue-700 transition-colors",
					"Back to Dashboard"
				}
			}
		}
	})()
}
