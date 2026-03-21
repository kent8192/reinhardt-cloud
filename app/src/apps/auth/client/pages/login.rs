//! Login page with username and password form.
//!
//! Uses `form!` macro for declarative form rendering with `page!` for layout.
//! Form submission is handled via standard HTML form POST to
//! the server function endpoint.

use reinhardt::pages::component::Page;
use reinhardt::pages::form;
use reinhardt::pages::page;

/// Render the login page.
pub fn login_page() -> Page {
	let login_form = form! {
		name: LoginForm,
		action: "/api/server_fn/login",
		method: Post,
		class: "space-y-4",

		fields: {
			username: CharField {
				required,
				label: "Username",
				placeholder: "Enter your username",
				class: "w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500",
				label_class: "block text-sm font-medium text-gray-700 mb-1",
			},
			password: PasswordField {
				required,
				label: "Password",
				placeholder: "Enter your password",
				class: "w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500",
				label_class: "block text-sm font-medium text-gray-700 mb-1",
			},
		},
	};

	let form_view = login_form.into_page();

	page!(|form_view: Page| {
		div {
			{ form_view }
			div {
				class: "pt-2",
				button {
					r#type: "submit",
					form: "login-form",
					class: "w-full py-2 px-4 bg-blue-600 text-white text-sm font-medium rounded-md hover:bg-blue-700 transition-colors",
					"Sign in"
				}
			}
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
	})(form_view)
}
