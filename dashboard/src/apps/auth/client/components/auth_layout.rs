//! Centered authentication layout wrapper.
//!
//! Provides a full-height centered card layout with Reinhardt Cloud branding,
//! used as a shared shell for login and register pages.

use reinhardt::pages::component::Page;
use reinhardt::pages::page;

/// Render a centered authentication layout with a branded card.
///
/// The `title` is shown below the Reinhardt Cloud header and `form_content`
/// is the page-specific form view rendered inside the card body.
pub fn auth_layout(title: &str, form_content: Page) -> Page {
	let title = title.to_string();
	page!(|title: String, form_content: Page| {
		div {
			class: "min-h-screen flex items-center justify-center bg-gray-50",
			div {
				class: "w-full max-w-md",
				div {
					class: "text-center mb-8",
					h1 {
						class: "text-3xl font-bold text-blue-600",
						"Reinhardt Cloud"
					}
					p {
						class: "text-sm text-gray-500 mt-1",
						"Cloud Platform"
					}
				}
				div {
					class: "bg-white rounded-lg border border-gray-200 shadow-sm p-8",
					h2 {
						class: "text-xl font-semibold text-gray-800 mb-6 text-center",
						{ title }
					}
					{ form_content }
				}
			}
		}
	})(title, form_content)
}
