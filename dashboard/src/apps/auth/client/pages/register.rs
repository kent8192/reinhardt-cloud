//! Registration page backed by the named `RegisterRequest` ClientForm DTO.

#[cfg(test)]
#[cfg(all(test, native))]
use std::future::Future;

use reinhardt::pages::component::Page;
use reinhardt::pages::event::{InputEvent, SubmitEvent};
use reinhardt::pages::page;
use reinhardt::pages::prelude::{
	Action, Callback, FieldError, FormState, Signal, UseFormAsyncSubmitOutcome, UseFormReturn,
	use_action, use_form, use_router,
};
use reinhardt::pages::server_fn::ServerFnError;
use reinhardt::{EmailValidator, Validator};

use crate::apps::auth::client::components::oauth_buttons;
use crate::apps::auth::serializers::RegisterRequest;
use crate::apps::auth::serializers::register::{
	RegisterRequestClientForm, RegisterRequestClientFormField,
};
use crate::shared::AuthResponse;
use crate::shared::client::routes::route_href;

#[derive(Clone)]
struct RegisterFormView {
	state: FormState<RegisterRequestClientFormField>,
	action: Action<UseFormAsyncSubmitOutcome<AuthResponse>, ServerFnError>,
	username: Signal<String>,
	email: Signal<String>,
	password: Signal<String>,
	email_input: Callback<InputEvent, ()>,
	password_input: Callback<InputEvent, ()>,
}

fn register_field_error(
	state: FormState<RegisterRequestClientFormField>,
	field: RegisterRequestClientFormField,
) -> Page {
	Page::reactive(move || {
		state
			.field_errors
			.get()
			.get(&field)
			.map(|error| {
				page!(|message: String| {
					p {
						class: "mt-1 text-xs font-medium text-red-700",
						role: "alert",
						{ message }
					}
				})(error.message().to_owned())
			})
			.unwrap_or(Page::Empty)
	})
}

fn register_form_error(state: FormState<RegisterRequestClientFormField>) -> Page {
	Page::reactive(move || {
		state
			.form_error
			.get()
			.or_else(|| state.submit_error.get())
			.map(|message| {
				page!(|message: String| {
					div {
						class: "rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm font-medium text-red-700",
						role: "alert",
						{ message }
					}
				})(message)
			})
			.unwrap_or(Page::Empty)
	})
}

fn render_register_form(view: RegisterFormView) -> Page {
	let RegisterFormView {
		state,
		action,
		username,
		email,
		password,
		email_input,
		password_input,
	} = view;
	let submit = Callback::new(move |event: SubmitEvent| {
		event.prevent_default();
		action.dispatch(());
	});
	let form_error = register_form_error(state.clone());
	let username_error =
		register_field_error(state.clone(), RegisterRequestClientFormField::Username);
	let email_error = register_field_error(state.clone(), RegisterRequestClientFormField::Email);
	let password_error =
		register_field_error(state.clone(), RegisterRequestClientFormField::Password);

	Page::reactive(move || {
		let is_submitting = state.is_submitting.get();
		let email_value = email.get();
		let password_value = password.get();
		let submit_label = if is_submitting {
			"Creating account..."
		} else {
			"Create account"
		};
		page!(|form_error: Page,
		 submit: Callback<SubmitEvent, ()>,
		 username: Signal<String>,
		 email: Signal<String>,
		 password: Signal<String>,
		 email_input: Callback<InputEvent, ()>,
		 password_input: Callback<InputEvent, ()>,
		 email_value: String,
		 password_value: String,
		 username_error: Page,
		 email_error: Page,
		 password_error: Page,
		 is_submitting: bool,
		 submit_label: &'static str| {
			form {
				class: "rc-form-stack",
				@submit: submit,
				{ form_error }
				div {
					class: "rc-field",
					label {
						class: "rc-label",
						r#for: "register-username",
						"Username"
					}
					input {
						id: "register-username",
						aria_label: "Username",
						aria_describedby: "register-username-error",
						class: "rc-input",
						type: "text",
						autocomplete: "username",
						maxlength: 32,
						placeholder: "Choose a username",
						bind: username,
					}
					div {
						id: "register-username-error",
						{ username_error }
					}
				}
				div {
					class: "rc-field",
					label {
						class: "rc-label",
						r#for: "register-email",
						"Email"
					}
					input {
						id: "register-email",
						aria_label: "Email",
						aria_describedby: "register-email-error",
						class: "rc-input",
						type: "email",
						autocomplete: "email",
						maxlength: 254,
						placeholder: "Enter your email",
						value: email_value,
						@input: email_input,
					}
					div {
						id: "register-email-error",
						{ email_error }
					}
				}
				div {
					class: "rc-field",
					label {
						class: "rc-label",
						r#for: "register-password",
						"Password"
					}
					input {
						id: "register-password",
						aria_label: "Password",
						aria_describedby: "register-password-error",
						class: "rc-input",
						type: "password",
						autocomplete: "new-password",
						maxlength: 128,
						placeholder: "Create a password (min 8 characters)",
						value: password_value,
						@input: password_input,
					}
					div {
						id: "register-password-error",
						{ password_error }
					}
				}
				button {
					type: "submit",
					class: "btn-primary min-h-11 w-full text-base",
					disabled: is_submitting,
					{ submit_label }
				}
			}
		})(
			form_error.clone(),
			submit,
			username,
			email,
			password,
			email_input,
			password_input,
			email_value,
			password_value,
			username_error.clone(),
			email_error.clone(),
			password_error.clone(),
			is_submitting,
			submit_label,
		)
	})
}

// Workaround for kent8192/reinhardt-web#6159 (tracked in
// kent8192/reinhardt-cloud#877). Remove this workaround when DTO-derived
// ClientForm validation supports wasm32.
//
// Ideal implementation (without workaround):
//   #[client_form(validate, server_fn = crate::apps::auth::server_fn::register::register)]
//   struct RegisterRequest { /* fields */ }
fn client_validated_register_request<Deps>(
	runtime: &UseFormReturn<RegisterRequestClientForm, Deps>,
) -> Option<RegisterRequest>
where
	Deps: Clone + PartialEq + 'static,
{
	runtime.clear_errors();
	let request: RegisterRequest = RegisterRequestClientForm::to_request(runtime);
	let mut valid = true;

	if request.username.chars().count() < 3 {
		runtime.set_error(
			RegisterRequestClientFormField::Username,
			FieldError::new("Username must be at least 3 characters"),
		);
		valid = false;
	} else if request.username.chars().count() > 32 {
		runtime.set_error(
			RegisterRequestClientFormField::Username,
			FieldError::new("Username must be 32 characters or fewer"),
		);
		valid = false;
	}

	if request.email.is_empty() {
		runtime.set_error(
			RegisterRequestClientFormField::Email,
			FieldError::new("Email is required"),
		);
		valid = false;
	} else if request.email.chars().count() > 254 {
		runtime.set_error(
			RegisterRequestClientFormField::Email,
			FieldError::new("Email must be 254 characters or fewer"),
		);
		valid = false;
	} else if EmailValidator::new()
		.validate(request.email.as_str())
		.is_err()
	{
		runtime.set_error(
			RegisterRequestClientFormField::Email,
			FieldError::new("Enter a valid email address"),
		);
		valid = false;
	}

	if request.password.chars().count() < 8 {
		runtime.set_error(
			RegisterRequestClientFormField::Password,
			FieldError::new("Password must be at least 8 characters"),
		);
		valid = false;
	} else if request.password.chars().count() > 128 {
		runtime.set_error(
			RegisterRequestClientFormField::Password,
			FieldError::new("Password must be 128 characters or fewer"),
		);
		valid = false;
	}

	valid.then_some(request)
}

#[cfg(all(test, native))]
async fn submit_register_with_runtime<Deps, Submit, Fut, Output>(
	runtime: &UseFormReturn<RegisterRequestClientForm, Deps>,
	submit: Submit,
) -> Result<UseFormAsyncSubmitOutcome<Output>, ServerFnError>
where
	Deps: Clone + PartialEq + 'static,
	Submit: FnOnce(RegisterRequest) -> Fut,
	Fut: Future<Output = Result<Output, ServerFnError>>,
{
	let Some(request) = client_validated_register_request(runtime) else {
		return Ok(UseFormAsyncSubmitOutcome::ValidationFailed);
	};
	runtime.submit_server_fn(|| submit(request)).await
}

#[cfg(wasm)]
async fn submit_register_client_form<Deps>(
	form: &RegisterRequestClientForm,
	runtime: &UseFormReturn<RegisterRequestClientForm, Deps>,
) -> Result<UseFormAsyncSubmitOutcome<AuthResponse>, ServerFnError>
where
	Deps: Clone + PartialEq + 'static,
{
	if client_validated_register_request(runtime).is_none() {
		return Ok(UseFormAsyncSubmitOutcome::ValidationFailed);
	}
	form.submit(runtime).await
}

#[cfg(not(wasm))]
async fn submit_register_client_form<Deps>(
	_form: &RegisterRequestClientForm,
	runtime: &UseFormReturn<RegisterRequestClientForm, Deps>,
) -> Result<UseFormAsyncSubmitOutcome<AuthResponse>, ServerFnError>
where
	Deps: Clone + PartialEq + 'static,
{
	let _ = client_validated_register_request(runtime);
	Ok(UseFormAsyncSubmitOutcome::ValidationFailed)
}

/// Render the registration page inside the shared auth layout.
#[reinhardt::pages::component("/register", name = "auth:register_page")]
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
	let register_state = register_runtime.form_state();
	let username = register_runtime.watch_field::<String>(register_form.username_field());
	let email = register_runtime.watch_field::<String>(register_form.email_field());
	let password = register_runtime.watch_field::<String>(register_form.password_field());
	let email_input = {
		Callback::new(move |event: InputEvent| {
			let Ok(value) = event.value() else {
				return;
			};
			email.set(value);
		})
	};
	let password_input = {
		Callback::new(move |event: InputEvent| {
			let Ok(value) = event.value() else {
				return;
			};
			password.set(value);
		})
	};
	let submit_form = register_form.clone();
	let submit_runtime = register_runtime.clone();
	let register_action = use_action(move |(): ()| {
		let form = submit_form.clone();
		let runtime = submit_runtime.clone();
		async move { submit_register_client_form(&form, &runtime).await }
	});
	let form_view = render_register_form(RegisterFormView {
		state: register_state,
		action: register_action,
		username,
		email,
		password,
		email_input,
		password_input,
	});
	let oauth_buttons = oauth_buttons();
	page!(|form_view: Page, oauth_buttons: Page, login_href: String| {
		div {
			class: "rc-app flex items-center justify-center px-4",
			div {
				class: "w-full max-w-md",
				div {
					class: "text-center mb-8",
					p {
						class: "rc-kicker mb-2",
						"Control plane"
					}
					h1 {
						class: "text-3xl font-semibold text-ink-950",
						"Reinhardt Cloud"
					}
					p {
						class: "rc-muted mt-1",
						"Cloud Platform"
					}
				}
				div {
					class: "rc-panel-pad p-8",
					h2 {
						class: "text-xl font-semibold text-ink-950 mb-6 text-center",
						"Create your account"
					}
					{ form_view }
					{ oauth_buttons }
					div {
						class: "mt-6 text-center text-sm text-ink-600",
						"Already have an account? " a {
							href: login_href,
							class: "font-semibold text-control-700 underline-offset-4 hover:underline",
							"Sign in"
						}
					}
				}
			}
		}
	})(form_view, oauth_buttons, login_href)
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
	async fn register_client_gate_blocks_server_dispatch_for_invalid_input() {
		// Arrange
		let scope = ReactiveScope::new();
		let runtime = scope.enter(|| {
			let form = RegisterRequestClientForm::new();
			use_form(&form).build()
		});
		let submit_calls = Rc::new(Cell::new(0));
		let submit_calls_for_submit = Rc::clone(&submit_calls);

		// Act
		let outcome = submit_register_with_runtime(&runtime, move |_| {
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
			Some("Username must be at least 3 characters")
		);
	}
}
