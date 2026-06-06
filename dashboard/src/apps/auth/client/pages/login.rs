//! Login page with username and password form.
//!
//! Uses `form!` for the static field/server_fn definition. Login success is
//! persisted by the server-side session cookie, then the form redirects to the
//! dashboard route.

use reinhardt::pages::component::Page;
use reinhardt::pages::form;
use reinhardt::pages::page;

use crate::apps::auth::server::login::login;

/// Render the login page.
pub fn login_page() -> Page {
	let login_form = form! {
		name: LoginForm,
		server_fn: login,
		method: Post,
		class: "space-y-4",
		redirect_on_success: "/",
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
			submit: SubmitButton {
				label: "Sign in",
				class: "btn-primary w-full py-2.5 text-base"
			},
		},
	};
	let form_view = login_form.into_page();

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
						"Sign in to your account"
					}
					{ form_view }
					div {
						id: "oauth-login-providers",
					}
					div {
						class: "mt-6 text-center text-sm text-gray-600",
						"Don't have an account? " a {
							href: "/register".to_string(),
							class: "text-blue-600 font-medium hover:underline",
							"Create one"
						}
					}
				}
			}
		}
	})(form_view)
}
