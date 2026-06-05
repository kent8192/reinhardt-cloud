//! Register page with username, email, and password form.
//!
//! Uses `form!` for the static field/server_fn definition. Registration creates
//! an inactive account and sends a verification email; it does not establish a
//! login session.

use reinhardt::pages::component::Page;
use reinhardt::pages::form;
use reinhardt::pages::page;

use crate::apps::auth::client::components::{auth_layout, oauth_buttons};
use crate::apps::auth::server::register::register;

/// Render the registration page inside the shared auth layout.
pub fn register_page() -> Page {
	let register_form = form! {
		name: RegisterForm,
		server_fn: register,
		method: Post,
		class: "space-y-4",
		redirect_on_success: "/login",
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
			submit: SubmitButton {
				label: "Create account",
				class: "btn-primary w-full py-2.5 text-base"
			},
		},
	};
	let form_view = register_form.into_page();
	let oauth_view = oauth_buttons("Sign up");

	let content = page!(|form_view: Page, oauth_view: Page| {
		div {
			{ form_view }
			{ oauth_view }
			div {
				class: "mt-6 text-center text-sm text-gray-600",
				"Already have an account? " a {
					href: "/login".to_string(),
					class: "text-blue-600 font-medium hover:underline",
					"Sign in"
				}
			}
		}
	})(form_view, oauth_view);

	auth_layout("Create your account", content)
}
