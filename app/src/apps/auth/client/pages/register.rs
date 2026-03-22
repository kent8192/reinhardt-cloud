//! Register page with username, email, and password form.
//!
//! Uses `page!` macro for declarative form rendering.

use reinhardt::pages::component::Page;
use reinhardt::pages::page;

use crate::apps::auth::client::components::auth_layout;

// Workaround for kent8192/reinhardt-web#2791 (tracked in reinhardt-cloud#109)
// Remove this workaround when the upstream issue is resolved.
//
// The `form!` macro internally references `reinhardt::pages::dom::submit_form`
// which is not publicly exported from the reinhardt facade, causing compilation
// errors on `wasm32-unknown-unknown`. Plain `page!` HTML forms are used instead.
//
// Ideal implementation (without workaround):
//   use reinhardt::pages::form;
//
//   let register_form = form! {
//       name: RegisterForm,
//       server_fn: server::register::register,
//       class: "space-y-4",
//       fields: {
//           username: CharField {
//               required, label: "Username",
//               placeholder: "Choose a username",
//           },
//           email: EmailField {
//               required, label: "Email",
//               placeholder: "Enter your email",
//           },
//           password: PasswordField {
//               required, label: "Password",
//               placeholder: "Create a password (min 8 characters)",
//           },
//       },
//   };
//   let form_view = register_form.into_page();

/// Render the registration page inside the shared auth layout.
pub fn register_page() -> Page {
	let is_required = true;
	let form_content = page!(|| {
		form {
			id: "register-form",
			action: "/api/server_fn/register",
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
					placeholder: "Choose a username",
					class: "w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500",
				}
			}

			div {
				label {
					r#for: "email",
					class: "block text-sm font-medium text-gray-700 mb-1",
					"Email"
				}
				input {
					r#type: "email",
					id: "email",
					name: "email",
					required: is_required,
					placeholder: "Enter your email",
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
					minlength: 8,
					placeholder: "Create a password (min 8 characters)",
					class: "w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500",
				}
			}

			div {
				class: "pt-2",
				button {
					r#type: "submit",
					class: "w-full py-2 px-4 bg-blue-600 text-white text-sm font-medium rounded-md hover:bg-blue-700 transition-colors",
					"Create Account"
				}
			}

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
	})();

	auth_layout("Create your account", form_content)
}
