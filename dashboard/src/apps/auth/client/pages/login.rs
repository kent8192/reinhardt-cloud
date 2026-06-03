//! Login page with username and password form.
//!
//! Uses `form!` macro for declarative form rendering with automatic
//! server function integration. On successful login, the auth state is
//! updated via `AuthState` and the user is redirected to the dashboard.

use reinhardt::pages::component::Page;
use reinhardt::pages::form;
use reinhardt::pages::page;

use crate::apps::auth::client::components::oauth_buttons;
use crate::apps::auth::server::login::login;

/// Render the login page.
pub fn login_page() -> Page {
	let login_form = form! {
		name: LoginForm,
		server_fn: login,
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
	let oauth_view = oauth_buttons("Sign in");

	let content = page!(|form_view: Page, oauth_view: Page| {
		div {
			{ form_view }
			{ oauth_view }
			div {
				class: "mt-6 text-center text-sm text-gray-600",
				"Don't have an account? " a {
					href: "/register".to_string(),
					class: "text-blue-600 font-medium hover:underline",
					"Create one"
				}
			}
		}
	})(form_view, oauth_view);

	crate::apps::auth::client::components::auth_layout("Sign in to your account", content)
}
