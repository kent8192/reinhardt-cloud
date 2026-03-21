//! Register page with username, email, and password form.
//!
//! Uses `form!` macro for declarative form rendering with `page!` for layout.

use reinhardt::pages::component::Page;
use reinhardt::pages::form;
use reinhardt::pages::page;

use crate::apps::auth::client::components::auth_layout;

/// Render the registration page inside the shared auth layout.
pub fn register_page() -> Page {
	let register_form = form! {
		name: RegisterForm,
		action: "/api/server_fn/register",
		method: Post,
		class: "space-y-4",

		fields: {
			username: CharField {
				required,
				label: "Username",
				placeholder: "Choose a username",
				class: "w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500",
				label_class: "block text-sm font-medium text-gray-700 mb-1",
			},
			email: EmailField {
				required,
				label: "Email",
				placeholder: "Enter your email",
				class: "w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500",
				label_class: "block text-sm font-medium text-gray-700 mb-1",
			},
			password: PasswordField {
				required,
				min_length: 8,
				label: "Password",
				placeholder: "Create a password (min 8 characters)",
				class: "w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500",
				label_class: "block text-sm font-medium text-gray-700 mb-1",
			},
		},
	};

	let form_view = register_form.into_page();

	let form_content = page!(|form_view: Page| {
		div {
			{ form_view }
			div {
				class: "pt-2",
				button {
					r#type: "submit",
					form: "register-form",
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
	})(form_view);

	auth_layout("Create your account", form_content)
}
