//! Login page with username and password form.
//!
//! Uses `form!` macro for declarative form rendering with automatic
//! server function integration.

use reinhardt::pages::component::Page;
use reinhardt::pages::form;
use reinhardt::pages::page;

use crate::apps::auth::server::login::login;

/// Render the login page.
pub fn login_page() -> Page {
	let login_form = form! {
		name: LoginForm,
		server_fn: login,
		class: "space-y-4",
		fields: {
			username: CharField {
				required,
				max_length: 150,
				label: "Username",
				placeholder: "Enter your username",
			},
			password: PasswordField {
				required,
				min_length: 8,
				label: "Password",
				placeholder: "Enter your password",
			},
		},
	};
	let form_view = login_form.into_page();

	let content = page!(|form_view: Page| {
		div {
			{ form_view }
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
	})(form_view);

	crate::apps::auth::client::components::auth_layout("Sign in to your account", content)
}
