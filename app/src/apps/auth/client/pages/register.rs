//! Register page with username, email, and password form.
//!
//! Uses plain `page!` macro for static HTML form rendering.
// Workaround: reinhardt-pages reactive Effect system panics with
// "RefCell already borrowed" on non-/ routes during WASM initialization.
// See: https://github.com/kent8192/reinhardt-web/issues/2667
// Scope: client.rs, auth/client/pages/login.rs, auth/client/pages/register.rs

use reinhardt::pages::component::Page;
use reinhardt::pages::page;

use crate::apps::auth::client::components::auth_layout;

/// Render the registration page inside the shared auth layout.
pub fn register_page() -> Page {
	let is_required = true;
	let form_content = page!(|is_required: bool| {
		div {
			form {
				method: "post",
				action: "/api/server_fn/register",
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
						placeholder: "Choose a username",
						class: "w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500",
					}
				}
				// Email field
				div {
					label {
						r#for: "email",
						class: "block text-sm font-medium text-gray-700 mb-1",
						"Email"
					}
					input {
						r#type: "email",
						name: "email",
						id: "email",
						placeholder: "Enter your email",
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
						// Boolean attribute requires a dynamic expression (page! macro rule)
						required: is_required,
						minlength: 8,
						placeholder: "Create a password (min 8 characters)",
						class: "w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500",
					}
				}
				// Submit button
				div {
					class: "pt-2",
					button {
						r#type: "submit",
						class: "w-full py-2 px-4 bg-blue-600 text-white text-sm font-medium rounded-md hover:bg-blue-700 transition-colors",
						"Create Account"
					}
				}
			}
			// Link to login page
			div {
				class: "mt-6 text-center text-sm text-gray-600",
				"Already have an account? "
				a {
					href: "/login",
					class: "text-blue-600 font-medium hover:underline",
					"Sign in"
				}
			}
		}
	})(is_required);

	auth_layout("Create your account", form_content)
}
