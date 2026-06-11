//! Register page with username, email, and password form.
//!
//! Uses `form!` for the static field/server_fn definition. Registration creates
//! an inactive account and sends a verification email; it does not establish a
//! login session.

use reinhardt::pages::component::Page;
use reinhardt::pages::form;
use reinhardt::pages::page;

use crate::apps::auth::client::components::oauth_buttons;
use crate::apps::auth::server_fn::register::register;
use crate::shared::client::routes::route_href;

/// Render the registration page inside the shared auth layout.
pub fn register_page() -> Page {
	let register_form = form! {
		name: RegisterForm,
		server_fn: register,
		method: Post,
		class: "rc-form-stack",
		success_url: |_form| route_href("auth:login_page", "/login"),
		fields: {
			username: CharField {
				required,
				max_length: 150,
				label: "Username",
				placeholder: "Choose a username",
				class: "rc-input",
			},
			email: EmailField {
				required,
				label: "Email",
				placeholder: "Enter your email",
				class: "rc-input",
			},
			password: PasswordField {
				required,
				min_length: 8,
				label: "Password",
				placeholder: "Create a password (min 8 characters)",
				class: "rc-input",
			},
			submit: SubmitButton {
				label: "Create account",
				class: "btn-primary min-h-11 w-full text-base"
			},
		},
	};
	let form_view = register_form.into_page();
	let oauth_buttons = oauth_buttons();
	let login_href = route_href("auth:login_page", "/login");

	page!(|form_view: Page, oauth_buttons: Page, login_href: String| {
		div {
			class: "rc-app flex items-center justify-center px-4",
			div {
				class: "w-full max-w-md",
				div {
					class: "text-center mb-8",
					p {
						class: "rc-kicker mb-2",
						"Control plane"
					}
					h1 {
						class: "text-3xl font-semibold text-ink-950",
						"Reinhardt Cloud"
					}
					p {
						class: "rc-muted mt-1",
						"Cloud Platform"
					}
				}
				div {
					class: "rc-panel-pad p-8",
					h2 {
						class: "text-xl font-semibold text-ink-950 mb-6 text-center",
						"Create your account"
					}
					{ form_view }
					{ oauth_buttons }
					div {
						class: "mt-6 text-center text-sm text-ink-600",
						"Already have an account? " a {
							href: login_href,
							class: "font-semibold text-control-700 underline-offset-4 hover:underline",
							"Sign in"
						}
					}
				}
			}
		}
	})(form_view, oauth_buttons, login_href)
}
