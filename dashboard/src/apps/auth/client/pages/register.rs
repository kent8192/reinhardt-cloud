//! Register page with username, email, and password form.
//!
//! Uses `form!` macro for declarative form rendering with automatic
//! server function integration. On successful registration, the auth state
//! is updated via `AuthState` and the user is redirected to the dashboard.

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
		},
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
			submit: SubmitButton { label: "Create account", class: "btn-primary w-full py-2.5 text-base" },
		},
		// Explicit CSRF wiring (reinhardt-web#3971) — reads the token from
		// the cookie/meta/input chain at submit time and routes it to the
		// server_fn's `csrf_token` parameter.
		strip_arguments: {
			csrf_token: ::reinhardt::reinhardt_pages::csrf::get_csrf_token().unwrap_or_default(),
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
				"Already have an account? "
				a {
					href: "/login",
					class: "text-blue-600 font-medium hover:underline",
					"Sign in"
				}
			}
		}
	})(form_view, oauth_view);

	auth_layout("Create your account", content)
}
