//! Login page with username and password form.
//!
//! Uses plain `page!` macro for static HTML form rendering.
//! Form submission is handled via standard HTML form POST to
//! the server function endpoint.
// Workaround: form! macro causes RefCell already borrowed panic
// when used with render_to_string() due to reactive Effect creation.
// See: https://github.com/kent8192/reinhardt-web/issues/TBD
// Scope: apps/auth/client/pages/login.rs, register.rs

use reinhardt::pages::component::Page;
use reinhardt::pages::page;

use crate::apps::auth::client::components::auth_layout;

/// Render the login page inside the shared auth layout.
pub fn login_page() -> Page {
	let form_content = page!(|| {
		div {
			form {
				method: "post",
				action: "/api/server_fn/login",
				class: "space-y-4",
				// Username field
				div {
					label {
						r#for: "username",
						class: "block text-sm font-medium text-gray-700 mb-1",
						"Username"
					}
					input {
						r#type: "text",
						name: "username",
						id: "username",
						placeholder: "Enter your username",
						class: "w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500",
					}
				}
				// Password field
				div {
					label {
						r#for: "password",
						class: "block text-sm font-medium text-gray-700 mb-1",
						"Password"
					}
					input {
						r#type: "password",
						name: "password",
						id: "password",
						placeholder: "Enter your password",
						class: "w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500",
					}
				}
				// Submit button
				div {
					class: "pt-2",
					button {
						r#type: "submit",
						class: "w-full py-2 px-4 bg-blue-600 text-white text-sm font-medium rounded-md hover:bg-blue-700 transition-colors",
						"Sign in"
					}
				}
			}
			// Link to register page
			div {
				class: "mt-6 text-center text-sm text-gray-600",
				"Don't have an account? "
				a {
					href: "/register",
					class: "text-blue-600 font-medium hover:underline",
					"Create one"
				}
			}
		}
	})();

	auth_layout("Sign in to your account", form_content)
}
