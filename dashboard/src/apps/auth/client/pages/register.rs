//! Registration page backed by the named `RegisterRequest` ClientForm DTO.

use reinhardt::pages::component;
use reinhardt::pages::component::Page;
use reinhardt::pages::event::SubmitEvent;
use reinhardt::pages::page;
#[cfg(test)]
use reinhardt::pages::prelude::FieldError;
#[cfg(all(test, native))]
use reinhardt::pages::prelude::UseFormAsyncSubmitOutcome;
use reinhardt::pages::prelude::{
	Callback, FormState, UseFormReturn, use_callback, use_form, use_router,
};
use reinhardt::pages::reactive::ExplicitDeps;
use reinhardt::pages::server_fn::ServerFnError;

use crate::apps::auth::client::components::{auth_layout, oauth_buttons};
use crate::apps::auth::client::style::STYLES;
#[cfg(test)]
use crate::apps::auth::serializers::RegisterRequest;
use crate::apps::auth::serializers::register::{
	RegisterRequestClientForm, RegisterRequestClientFormField,
};
use crate::shared::client::routes::route_href;
use crate::shared::client::style::STYLES as SHARED_STYLES;

#[derive(Clone)]
struct RegisterFormView {
	runtime: UseFormReturn<RegisterRequestClientForm>,
	submit: Callback<SubmitEvent, ()>,
}

fn register_field_error(
	state: FormState<RegisterRequestClientFormField>,
	field: RegisterRequestClientFormField,
) -> Page {
	page!({
		{
			state
				.field_errors
				.get()
				.get(&field)
				.map(|error| {
					let message = error.message().to_owned();
					page!({
						p {
							class: STYLES.field_error(),
							role: "alert",
							{ message }
						}
					})
				})
				.unwrap_or(Page::Empty)
		}
	})
}

fn register_form_error(state: FormState<RegisterRequestClientFormField>) -> Page {
	page!({
		{
			state
				.form_error
				.get()
				.or_else(|| state.submit_error.get())
				.map(|message| {
					page!({
						div {
							class: STYLES.form_error(),
							role: "alert",
							{ message }
						}
					})
				})
				.unwrap_or(Page::Empty)
		}
	})
}

fn render_register_form(view: RegisterFormView) -> Page {
	let state = view.runtime.form_state();
	let form_error = register_form_error(state.clone());
	let username_error =
		register_field_error(state.clone(), RegisterRequestClientFormField::Username);
	let email_error = register_field_error(state.clone(), RegisterRequestClientFormField::Email);
	let password_error =
		register_field_error(state.clone(), RegisterRequestClientFormField::Password);

	page!({
		{
			let is_submitting = state.is_submitting.get();
			let submit_label = if is_submitting {
				"Creating account..."
			} else {
				"Create account"
			};
			page!({
				form {
					class: SHARED_STYLES.form_stack(),
					@submit: view.submit,
					{ form_error }
					div {
						class: SHARED_STYLES.field(),
						label {
							span { class: SHARED_STYLES.label(), "Username" }
							input {
								id: "register-username",
								name: "username",
								aria_label: "Username",
								aria_describedby: "register-username-error",
								class: SHARED_STYLES.input(),
								type: "text",
								autocomplete: "username",
								maxlength: 32,
								placeholder: "Choose a username",
								bind: view.runtime.field(RegisterRequestClientFormField::Username),
							}
						}
						div {
							id: "register-username-error",
							{ username_error }
						}
					}
					div {
						class: SHARED_STYLES.field(),
						label {
							span { class: SHARED_STYLES.label(), "Email" }
							input {
								id: "register-email",
								name: "email",
								aria_label: "Email",
								aria_describedby: "register-email-error",
								class: SHARED_STYLES.input(),
								type: "email",
								autocomplete: "email",
								maxlength: 254,
								placeholder: "Enter your email",
								bind: view.runtime.field(RegisterRequestClientFormField::Email),
							}
						}
						div {
							id: "register-email-error",
							{ email_error }
						}
					}
					div {
						class: SHARED_STYLES.field(),
						label {
							span { class: SHARED_STYLES.label(), "Password" }
							input {
								id: "register-password",
								name: "password",
								aria_label: "Password",
								aria_describedby: "register-password-error",
								class: SHARED_STYLES.input(),
								type: "password",
								autocomplete: "new-password",
								maxlength: 128,
								placeholder: "Create a password (min 8 characters)",
								bind: view.runtime.field(RegisterRequestClientFormField::Password),
							}
						}
						div {
							id: "register-password-error",
							{ password_error }
						}
					}
					button {
						type: "submit",
						class: SHARED_STYLES.button_primary() + STYLES.form_submit(),
						disabled: is_submitting,
						{ submit_label }
					}
				}
			})
		}
	})
}

/// Render the registration page inside the shared auth layout.
#[component("/register", name = "auth:register_page")]
pub fn register_page() -> Page {
	let register_form = RegisterRequestClientForm::new();
	let router = use_router();
	let login_href = route_href("auth:login_page", "/login");
	let submit_login_href = login_href.clone();
	let register_runtime = use_form(&register_form)
		.on_submit_success(move |runtime| {
			if router.replace(submit_login_href.clone()).is_err() {
				runtime.apply_server_error(&ServerFnError::application(
					"Account created, but navigation to sign in failed",
				));
			}
		})
		.build();
	let mutation = register_form.server_mutation(&register_runtime).build();
	let submit = use_callback(
		move |event: SubmitEvent| {
			event.prevent_default();
			RegisterRequestClientForm::normalize_values(&mutation.form());
			mutation.dispatch();
		},
		ExplicitDeps::from_node_ids([]),
	);
	let form_view = render_register_form(RegisterFormView {
		runtime: register_runtime,
		submit,
	});
	let oauth_buttons = oauth_buttons();
	let footer = page!({
		div {
			class: STYLES.auth_footer(),
			"Already have an account? " a {
				href: login_href,
				class: SHARED_STYLES.link(),
				"Sign in"
			}
		}
	});
	auth_layout(
		"Create your account",
		Page::fragment([form_view, oauth_buttons, footer]),
	)
}

#[cfg(test)]
mod tests {
	#[cfg(native)]
	use std::cell::Cell;
	#[cfg(native)]
	use std::rc::Rc;

	use reinhardt::pages::reactive::ReactiveScope;
	use rstest::rstest;

	use super::*;

	#[cfg(native)]
	#[rstest]
	fn register_server_mutation_is_inert_during_native_rendering() {
		ReactiveScope::run(|| {
			// Arrange
			let form = RegisterRequestClientForm::new();
			let runtime = use_form(&form).build();
			let mutation = form.server_mutation(&runtime).build();

			// Act
			let outcome = mutation.dispatch();

			// Assert
			assert_eq!(
				outcome,
				reinhardt::pages::MutationDispatchOutcome::UnsupportedTarget
			);
			assert_eq!(mutation.is_pending(), false);
			assert_eq!(runtime.form_state().is_submitting.get(), false);
		});
	}

	#[rstest]
	fn register_client_form_preserves_the_named_request_payload() {
		// Arrange
		let expected = RegisterRequest {
			username: "alice".to_string(),
			email: "alice@example.com".to_string(),
			password: "correct-horse-battery-staple".to_string(),
		};

		ReactiveScope::run(|| {
			let form = RegisterRequestClientForm::new().with_defaults(expected.clone());
			let runtime = use_form(&form).build();

			// Act
			let request = RegisterRequestClientForm::to_request(&runtime);

			// Assert
			assert_eq!(request, expected);
		});
	}

	#[rstest]
	fn register_client_form_routes_structured_server_errors_to_fields_and_global_error() {
		ReactiveScope::run(|| {
			// Arrange
			let form = RegisterRequestClientForm::new().with_defaults(RegisterRequest {
				username: "alice".to_string(),
				email: "alice@example.com".to_string(),
				password: "correct-horse-battery-staple".to_string(),
			});
			let runtime = use_form(&form).build();
			let error = ServerFnError::validation_with_message(
				"Please correct the submitted values",
				[
					("email", "This email is already registered"),
					(
						"registration_policy",
						"Registration is temporarily disabled",
					),
				],
			);

			// Act
			runtime.apply_server_error(&error);

			// Assert
			assert_eq!(
				runtime
					.get_field_state(RegisterRequestClientFormField::Email)
					.error
					.as_ref()
					.map(FieldError::message),
				Some("This email is already registered")
			);
			assert_eq!(
				runtime.form_state().form_error.get(),
				Some(
					"Please correct the submitted values\nregistration_policy: Registration is temporarily disabled"
						.to_string()
				)
			);
		});
	}

	#[cfg(native)]
	#[rstest]
	#[tokio::test]
	async fn register_generated_client_form_blocks_server_dispatch_for_invalid_input() {
		// Arrange
		let scope = ReactiveScope::new();
		let runtime = scope.enter(|| {
			let form = RegisterRequestClientForm::new();
			use_form(&form).build()
		});
		let submit_calls = Rc::new(Cell::new(0));
		let submit_calls_for_submit = Rc::clone(&submit_calls);

		// Act
		let outcome = runtime
			.submit_server_fn(move || {
				submit_calls_for_submit.set(submit_calls_for_submit.get() + 1);
				async { Ok::<_, ServerFnError>(()) }
			})
			.await
			.expect("client validation rejection should be a submit outcome");

		// Assert
		assert_eq!(outcome, UseFormAsyncSubmitOutcome::ValidationFailed);
		assert_eq!(submit_calls.get(), 0);
		assert_eq!(
			runtime
				.get_field_state(RegisterRequestClientFormField::Username)
				.error
				.as_ref()
				.map(FieldError::message),
			Some("Length too short: 0 (minimum: 3)")
		);
	}
}
