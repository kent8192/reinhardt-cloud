//! Register page with username, email, and password form.
//!
//! Uses the `form!` macro for type-safe form handling with reactive
//! loading and error states. On successful registration, stores the
//! auth token in global state and navigates to the dashboard.

use reinhardt::pages::component::Page;
use reinhardt::pages::{form, page};

use crate::apps::auth::client::components::auth_layout;
use crate::apps::auth::server::register::register;
use crate::client::state::with_app_state_mut;

/// Render the register page inside the shared auth layout.
pub fn register_page() -> Page {
	let register_form = form! {
		name: RegisterForm,
		server_fn: register,
		method: Post,
		class: "space-y-4",

		state: { loading, error },

		fields: {
			username: CharField {
				required,
				max_length: 150,
				label: "Username",
				placeholder: "Choose a username",
				class: "w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500",
			},
			email: CharField {
				required,
				max_length: 254,
				label: "Email",
				placeholder: "you@example.com",
				class: "w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500",
			},
			password: CharField {
				required,
				min_length: 8,
				widget: PasswordInput,
				label: "Password",
				placeholder: "At least 8 characters",
				class: "w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500",
			},
		},

		client_validators: {
			email: [
				"value.includes('@')" => "Please enter a valid email address",
			],
			password: [
				"value.length >= 8" => "Password must be at least 8 characters",
			],
		},

		watch: {
			// Error banner displayed when registration fails
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
							{ if is_loading { "Creating account..." } else { "Create account" } }
						}
					}
				})(is_loading)
			},
			// Navigate to dashboard on successful registration
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

	let form_content = page!(|register_form: Page| {
		div {
			{ register_form }
			// Link to login page
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
	})(register_form.into_view());

	auth_layout("Create your account", form_content)
}
