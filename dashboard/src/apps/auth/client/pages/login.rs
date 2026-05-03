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
use crate::client::url::url_for_spa;

/// Render the login page.
pub fn login_page() -> Page {
	let login_form = form! {
		name: LoginForm,
		server_fn: login,
		class: "space-y-4",
		on_success: |result: crate::shared::AuthResponse| {
			use reinhardt::pages::auth::{AuthData, auth_state};

			// Update reactive auth state for UI components
			if let Some(ref user) = result.user {
				auth_state().update(AuthData {
					is_authenticated: true,
					// UUID-based user IDs cannot be represented as i64;
					// use username and email for client-side identification.
					user_id: None,
					username: Some(user.username.clone()),
					email: Some(user.email.clone()),
					..Default::default()
				});
			}

			// Redirect to the dashboard via the SPA URL resolver. Mirrors
			// the hard reload that `redirect_on_success` would emit so the
			// new session cookie is reloaded server-side.
			if let Some(window) = web_sys::window() {
				let _ = window
					.location()
					.set_href(&url_for_spa("dashboard:home"));
			}
		},
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
			submit: SubmitButton { label: "Sign in", class: "btn-primary w-full py-2.5 text-base" },
		},
		// Explicit CSRF wiring (reinhardt-web#3971) — reads the token from
		// the cookie/meta/input chain at submit time and routes it to the
		// server_fn's `csrf_token` parameter.
		strip_arguments: {
			csrf_token: ::reinhardt::reinhardt_pages::csrf::get_csrf_token().unwrap_or_default(),
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
				"Don't have an account? "
				a {
					href: url_for_spa("auth:register_page"),
					class: "text-blue-600 font-medium hover:underline",
					"Create one"
				}
			}
		}
	})(form_view, oauth_view);

	crate::apps::auth::client::components::auth_layout("Sign in to your account", content)
}
