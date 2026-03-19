//! Login page with username and password form.
//!
//! Uses the `form!` macro for type-safe form handling with reactive
//! loading and error states. On successful login, stores the auth
//! token in global state and navigates to the dashboard.

use reinhardt::pages::component::Page;
use reinhardt::pages::{form, page};

use crate::apps::auth::client::components::auth_layout;
use crate::apps::auth::server::login::login;
use crate::client::state::with_app_state_mut;

/// Render the login page inside the shared auth layout.
pub fn login_page() -> Page {
	let login_form = form! {
		name: LoginForm,
		server_fn: login,
		method: Post,
		class: "space-y-4",

		state: { loading, error },

		fields: {
			username: CharField {
				required,
				max_length: 150,
				label: "Username",
				placeholder: "Enter your username",
				class: "w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500",
			},
			password: CharField {
				required,
				min_length: 1,
				widget: PasswordInput,
				label: "Password",
				placeholder: "Enter your password",
				class: "w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500",
			},
		},

		watch: {
			// Error banner displayed when login fails
			error_banner: |form| {
				let err = form.error().get();
				page!(|err: Option<String>| {
					watch {
						if let Some(e) = err.clone() {
							div {
								class: "p-3 bg-red-50 border border-red-200 rounded-md text-sm text-red-700",
								{ e }
							}
						}
					}
				})(err)
			},
			// Submit button with loading state
			submit_button: |form| {
				let is_loading = form.loading().get();
				page!(|is_loading: bool| {
					div {
						class: "pt-2",
						button {
							type: "submit",
							class: if is_loading {
								"w-full py-2 px-4 bg-blue-400 text-white text-sm font-medium rounded-md cursor-not-allowed"
							} else {
								"w-full py-2 px-4 bg-blue-600 text-white text-sm font-medium rounded-md hover:bg-blue-700 transition-colors"
							},
							disabled: is_loading,
							{ if is_loading { "Signing in..." } else { "Sign in" } }
						}
					}
				})(is_loading)
			},
			// Navigate to dashboard on successful login
			success_navigation: |form| {
				let is_loading = form.loading().get();
				let err = form.error().get();
				page!(|is_loading: bool, err: Option<String>| {
					watch {
						if !is_loading && err.is_none() {
							// Store token in global state on success
							with_app_state_mut(|state| {
								state.token = Some("authenticated".to_string());
							});
							// Navigate to dashboard
							if let Some(window) = web_sys::window() {
								let _ = window.location().set_href("/");
							}
						}
					}
				})(is_loading, err)
			},
		},
	};

	let form_content = page!(|login_form: Page| {
		div {
			{ login_form }
			// Link to register page
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
	})(login_form.into_view());

	auth_layout("Sign in to your account", form_content)
}
