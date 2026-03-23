//! Register page with username, email, and password form.
//!
//! Uses `form!` macro for declarative form rendering with automatic
//! server function integration.

use reinhardt::pages::component::Page;
use reinhardt::pages::form;
use reinhardt::pages::page;

use crate::apps::auth::client::components::auth_layout;
use crate::apps::auth::server::register::register;

/// Render the registration page inside the shared auth layout.
pub fn register_page() -> Page {
	let register_form = form! {
		name: RegisterForm,
		server_fn: register,
		class: "space-y-4",
		fields: {
			username: CharField {
				required,
				max_length: 150,
				label: "Username",
				placeholder: "Choose a username",
			},
			email: EmailField {
				required,
				label: "Email",
				placeholder: "Enter your email",
			},
			password: PasswordField {
				required,
				min_length: 8,
				label: "Password",
				placeholder: "Create a password (min 8 characters)",
			},
		},
	};
	let form_view = register_form.into_page();

	let content = page!(|form_view: Page| {
		div {
			{ form_view }
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
	})(form_view);

	auth_layout("Create your account", content)
}
