//! Login page with username and password form.
//!
//! Uses `form!` for the static field/server_fn definition. Login success is
//! persisted by the server-side session cookie, then the form redirects to the
//! dashboard route.

use reinhardt::pages::component::Page;
use reinhardt::pages::form;
use reinhardt::pages::page;

use crate::apps::auth::client::components::oauth_buttons;
use crate::apps::auth::client::pages::auth_href;
use crate::apps::auth::server_fn::login::login;

/// Render the login page.
pub fn login_page() -> Page {
	let login_form = form! {
		name: LoginForm,
		server_fn: login,
		method: Post,
		class: "rc-form-stack",
		redirect_on_success: "/",
		fields: {
			username: CharField {
				required,
				max_length: 150,
				label: "Username",
				placeholder: "Enter your username",
				class: "rc-input",
			},
			password: PasswordField {
				required,
				min_length: 8,
				label: "Password",
				placeholder: "Enter your password",
				class: "rc-input",
			},
			submit: SubmitButton {
				label: "Sign in",
				class: "btn-primary min-h-11 w-full text-base"
			},
		},
	};
	let form_view = login_form.into_page();
	let oauth_buttons = oauth_buttons();
	let register_href = auth_href("auth:register_page", "/register");
	page!(|form_view: Page, oauth_buttons: Page, register_href: String| {
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
						"Sign in to your account"
					}
					{ form_view }
					{ oauth_buttons }
					div {
						class: "mt-6 text-center text-sm text-ink-600",
						"Don't have an account? " a {
							href: register_href,
							class: "font-semibold text-control-700 underline-offset-4 hover:underline",
							"Create one"
						}
					}
				}
			}
		}
	})(form_view, oauth_buttons, register_href)
}
