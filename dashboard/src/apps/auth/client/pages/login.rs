//! Login page backed by the named `LoginRequest` ClientForm DTO.

use reinhardt::pages::component;
use reinhardt::pages::component::Page;
use reinhardt::pages::event::SubmitEvent;
#[cfg(test)]
use reinhardt::pages::prelude::FieldError;
#[cfg(all(test, native))]
use reinhardt::pages::prelude::UseFormAsyncSubmitOutcome;
use reinhardt::pages::prelude::{Callback, FormState, UseFormReturn, use_callback, use_form};
use reinhardt::pages::reactive::ExplicitDeps;
use reinhardt::pages::server_fn::ServerFnError;
use reinhardt::pages::{NavigationType, navigate_or_reload, page};

use crate::apps::auth::client::components::{auth_layout, oauth_buttons};
use crate::apps::auth::client::style::STYLES;
#[cfg(test)]
use crate::apps::auth::serializers::LoginRequest;
use crate::apps::auth::serializers::login::{LoginRequestClientForm, LoginRequestClientFormField};
use crate::shared::client::routes::route_href;
use crate::shared::client::style::STYLES as SHARED_STYLES;

#[derive(Clone)]
struct LoginFormView {
	runtime: UseFormReturn<LoginRequestClientForm>,
	submit: Callback<SubmitEvent, ()>,
}

fn login_field_error(
	state: FormState<LoginRequestClientFormField>,
	field: LoginRequestClientFormField,
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

fn login_form_error(state: FormState<LoginRequestClientFormField>) -> Page {
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

fn render_login_form(view: LoginFormView) -> Page {
	let state = view.runtime.form_state();
	let form_error = login_form_error(state.clone());
	let username_error = login_field_error(state.clone(), LoginRequestClientFormField::Username);
	let password_error = login_field_error(state.clone(), LoginRequestClientFormField::Password);

	page!({
		{
			let is_submitting = state.is_submitting.get();
			let submit_label = if is_submitting {
				"Signing in..."
			} else {
				"Sign in"
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
								id: "login-username",
								name: "username",
								aria_label: "Username",
								aria_describedby: "login-username-error",
								class: SHARED_STYLES.input(),
								type: "text",
								autocomplete: "username",
								maxlength: 150,
								placeholder: "Enter your username",
								bind: view.runtime.field(LoginRequestClientFormField::Username),
							}
						}
						div {
							id: "login-username-error",
							{ username_error }
						}
					}
					div {
						class: SHARED_STYLES.field(),
						label {
							span { class: SHARED_STYLES.label(), "Password" }
							input {
								id: "login-password",
								name: "password",
								aria_label: "Password",
								aria_describedby: "login-password-error",
								class: SHARED_STYLES.input(),
								type: "password",
								autocomplete: "current-password",
								maxlength: 128,
								placeholder: "Enter your password",
								bind: view.runtime.field(LoginRequestClientFormField::Password),
							}
						}
						div {
							id: "login-password-error",
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

/// Render the login page.
#[component("/login", name = "auth:login_page")]
pub fn login_page() -> Page {
	let login_form = LoginRequestClientForm::new();
	let home_href = route_href("dashboard:home", "/");
	let submit_home_href = home_href.clone();
	let login_runtime = use_form(&login_form)
		.on_submit_success(move |runtime| {
			crate::shared::client::ws::disconnect_notifications();
			reinhardt::pages::auth::invalidate_authentication();
			if let Err(error) =
				navigate_or_reload(submit_home_href.clone(), NavigationType::Replace)
			{
				runtime.apply_server_error(&ServerFnError::application(format!(
					"Signed in, but navigation to the dashboard failed: {error}"
				)));
			}
		})
		.build();
	let mutation = login_form.server_mutation(&login_runtime).build();
	let submit = use_callback(
		move |event: SubmitEvent| {
			event.prevent_default();
			mutation.dispatch();
		},
		ExplicitDeps::from_node_ids([]),
	);
	let form_view = render_login_form(LoginFormView {
		runtime: login_runtime,
		submit,
	});
	let oauth_buttons = oauth_buttons();
	let register_href = route_href("auth:register_page", "/register");
	let footer = page!({
		div {
			class: STYLES.auth_footer(),
			"Don't have an account? " a {
				href: register_href,
				class: SHARED_STYLES.link(),
				"Create one"
			}
		}
	});
	auth_layout(
		"Sign in to your account",
		Page::fragment([form_view, oauth_buttons, footer]),
	)
}

#[cfg(test)]
mod tests {
	#[cfg(native)]
	use std::cell::{Cell, RefCell};
	#[cfg(native)]
	use std::rc::Rc;

	#[cfg(native)]
	use crate::apps::auth::server_fn::me::me;
	#[cfg(native)]
	use reinhardt::pages::prelude::{QueryHandle, QueryOptions, QueryStatus, use_query};
	use reinhardt::pages::reactive::ReactiveScope;
	#[cfg(native)]
	use reinhardt::pages::testing::component::{Role, render};
	use rstest::rstest;

	#[cfg(native)]
	use crate::shared::UserInfo;

	use super::*;

	#[cfg(native)]
	#[rstest]
	fn login_server_mutation_is_inert_during_native_rendering() {
		ReactiveScope::run(|| {
			// Arrange
			let form = LoginRequestClientForm::new();
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
	fn login_client_form_preserves_the_named_request_payload() {
		// Arrange
		let expected = LoginRequest {
			username: "alice".to_string(),
			password: "correct-horse-battery-staple".to_string(),
		};

		ReactiveScope::run(|| {
			let form = LoginRequestClientForm::new().with_defaults(expected.clone());
			let runtime = use_form(&form).build();

			// Act
			let request = LoginRequestClientForm::to_request(&runtime);

			// Assert
			assert_eq!(request, expected);
		});
	}

	#[rstest]
	fn login_client_form_routes_structured_server_errors_to_fields_and_global_error() {
		ReactiveScope::run(|| {
			// Arrange
			let form = LoginRequestClientForm::new().with_defaults(LoginRequest {
				username: "alice".to_string(),
				password: "correct-horse-battery-staple".to_string(),
			});
			let runtime = use_form(&form).build();
			let error = ServerFnError::validation_with_message(
				"Please correct the submitted values",
				[
					("username", "This username is unavailable"),
					("account_policy", "This account cannot sign in"),
				],
			);

			// Act
			runtime.apply_server_error(&error);

			// Assert
			assert_eq!(
				runtime
					.get_field_state(LoginRequestClientFormField::Username)
					.error
					.as_ref()
					.map(FieldError::message),
				Some("This username is unavailable")
			);
			assert_eq!(
				runtime.form_state().form_error.get(),
				Some(
					"Please correct the submitted values\naccount_policy: This account cannot sign in"
						.to_string()
				)
			);
		});
	}

	#[cfg(native)]
	fn current_user_query_probe(
		user: UserInfo,
		fetches: Rc<Cell<u32>>,
		captured: Rc<RefCell<Option<QueryHandle<UserInfo, ServerFnError>>>>,
	) -> Page {
		let query = use_query(
			me::family().query((), move || {
				fetches.set(fetches.get() + 1);
				let user = user.clone();
				async move { Ok::<UserInfo, ServerFnError>(user) }
			}),
			QueryOptions::new(),
		);
		*captured.borrow_mut() = Some(query.clone());
		Page::fragment(vec![
			Page::reactive(move || {
				query
					.data()
					.map(|user| Page::text(user.username))
					.unwrap_or_else(|| Page::text("Loading"))
			}),
			page!({
				button {
					@click: |_| reinhardt::pages::auth::invalidate_authentication(),
					"Invalidate authentication"
				}
			}),
		])
	}

	#[cfg(native)]
	#[rstest]
	#[tokio::test]
	async fn login_invalidates_authentication_cache_before_navigation() {
		// Arrange
		let alice = UserInfo {
			id: "user-alice".to_owned(),
			username: "alice".to_owned(),
			email: "alice@example.com".to_owned(),
		};
		let fetches = Rc::new(Cell::new(0));
		let captured = Rc::new(RefCell::new(None));
		let first_document = render({
			let fetches = Rc::clone(&fetches);
			let captured = Rc::clone(&captured);
			move || current_user_query_probe(alice, fetches, captured)
		});
		first_document.settle().await;
		let query = captured
			.borrow_mut()
			.take()
			.expect("the mounted current-user query should be captured");
		assert_eq!(
			query.data().map(|user| user.username),
			Some("alice".to_owned())
		);

		// Act
		first_document
			.get_by_role(Role::Button, "Invalidate authentication")
			.click();

		// Assert
		assert_eq!(fetches.get(), 1);
		assert_eq!(query.data(), None);
		assert_eq!(query.snapshot().status, QueryStatus::Pending);
	}

	#[cfg(native)]
	#[rstest]
	#[tokio::test]
	async fn login_generated_client_form_blocks_server_dispatch_for_invalid_input() {
		// Arrange
		let scope = ReactiveScope::new();
		let runtime = scope.enter(|| {
			let form = LoginRequestClientForm::new();
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
				.get_field_state(LoginRequestClientFormField::Username)
				.error
				.as_ref()
				.map(FieldError::message),
			Some("Length too short: 0 (minimum: 1)")
		);
	}
}
