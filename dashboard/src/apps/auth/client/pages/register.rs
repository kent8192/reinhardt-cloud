//! Register page with username, email, and password form.
//!
//! Uses `form!` for the static field/server_fn definition. Registration creates
//! an inactive account and sends a verification email; it does not establish a
//! login session.

use reinhardt::pages::component::Page;
use reinhardt::pages::form;
use reinhardt::pages::page;

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

	page!(|form_view: Page| {
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
						"Create your account"
					}
					{ form_view }
					div {
						id: "oauth-register-providers",
					}
					div {
						class: "mt-6 text-center text-sm text-gray-600",
						"Already have an account? " a {
							href: "/login".to_string(),
							class: "text-blue-600 font-medium hover:underline",
							"Sign in"
						}
					}
				}
			}
		}
	})(form_view)
}
