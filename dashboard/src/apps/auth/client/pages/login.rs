//! Login page with username and password form.
//!
//! Uses `form!` macro for declarative form rendering with automatic
//! server function integration. On successful login, the auth state is
//! updated via `AuthState` and the user is redirected to the dashboard.

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
		redirect_on_success: "/",
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

			// Persist token to sessionStorage for subsequent server function calls
			if let Some(ref token) = result.token {
				if let Some(window) = web_sys::window() {
					if let Ok(Some(storage)) = window.session_storage() {
						let _ = storage.set_item("auth_token", token);
					}
				}
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
			submit: SubmitButton {
				label: "Sign in",
				class: "w-full bg-blue-600 text-white py-2 px-4 rounded-lg font-medium hover:bg-blue-700 transition-colors",
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
