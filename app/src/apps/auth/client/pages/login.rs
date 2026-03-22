//! Login page with username and password form.
//!
//! Uses `page!` macro for declarative form rendering.
//! Form submission is handled via standard HTML form POST to
//! the server function endpoint.

use reinhardt::pages::component::Page;
use reinhardt::pages::page;

// Workaround for kent8192/reinhardt-web#2791 (tracked in reinhardt-cloud#109)
// Remove this workaround when the upstream issue is resolved.
//
// The `form!` macro internally references `reinhardt::pages::dom::submit_form`
// which is not publicly exported from the reinhardt facade, causing compilation
// errors on `wasm32-unknown-unknown`. Plain `page!` HTML forms are used instead.
//
// Ideal implementation (without workaround):
//   use reinhardt::pages::form;
//   use crate::apps::auth::server::login::login;
//
//   let login_form = form! {
//       name: LoginForm,
//       server_fn: login,
//       class: "space-y-4",
//       redirect_on_success: "/",
//       fields: {
//           username: CharField {
//               required,
//               max_length: 150,
//               label: "Username",
//               placeholder: "Enter your username",
//           },
//           password: PasswordField {
//               required,
//               min_length: 8,
//               label: "Password",
//               placeholder: "Enter your password",
//           },
//       },
//   };
//   let form_view = login_form.into_page();

/// Render the login page.
pub fn login_page() -> Page {
	let is_required = true;
	let form_content = page!(|| {
		form {
			id: "login-form",
			action: "/api/server_fn/login",
			method: "post",
			class: "space-y-4",

			div {
				label {
					r#for: "username",
					class: "block text-sm font-medium text-gray-700 mb-1",
					"Username"
				}
				input {
					r#type: "text",
					id: "username",
					name: "username",
					required: is_required,
					placeholder: "Enter your username",
					class: "w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500",
				}
			}

			div {
				label {
					r#for: "password",
					class: "block text-sm font-medium text-gray-700 mb-1",
					"Password"
				}
				input {
					r#type: "password",
					id: "password",
					name: "password",
					required: is_required,
					placeholder: "Enter your password",
					class: "w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500",
				}
			}

			div {
				class: "pt-2",
				button {
					r#type: "submit",
					class: "w-full py-2 px-4 bg-blue-600 text-white text-sm font-medium rounded-md hover:bg-blue-700 transition-colors",
					"Sign in"
				}
			}

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
	})();

	// Wrap in auth layout
	crate::apps::auth::client::components::auth_layout("Sign in to your account", form_content)
}
