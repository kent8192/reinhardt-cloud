//! Login page backed by the named `LoginRequest` ClientForm DTO.

use reinhardt::pages::component::Page;
use reinhardt::pages::event::{InputEvent, SubmitEvent};
use reinhardt::pages::page;
#[cfg(test)]
use reinhardt::pages::prelude::FieldError;
use reinhardt::pages::prelude::{
	Action, Callback, FormState, QueryClient, Signal, UseFormAsyncSubmitOutcome, queries,
	use_action, use_form,
};
use reinhardt::pages::server_fn::ServerFnError;

use crate::apps::auth::client::components::oauth_buttons;
#[cfg(test)]
use crate::apps::auth::serializers::LoginRequest;
use crate::apps::auth::serializers::login::{LoginRequestClientForm, LoginRequestClientFormField};
use crate::apps::auth::server_fn::linked_accounts::list_linked_oauth_accounts;
use crate::apps::auth::server_fn::me::me;
use crate::apps::auth::server_fn::oauth_providers::list_oauth_providers;
use crate::apps::clusters::server_fn::list_clusters_for_current_org;
use crate::apps::deployments::server_fn::{
	deployment_logs_for_current_org, list_deployment_previews_for_current_org,
	list_deployments_for_current_org,
};
use crate::apps::github::server_fn::{
	get_github_onboarding_for_current_org, list_github_project_previews_for_current_org,
	list_github_repositories_for_current_org, list_github_repositories_for_installation,
};
use crate::shared::AuthResponse;
use crate::shared::client::routes::route_href;

#[derive(Clone)]
struct LoginFormView {
	state: FormState<LoginRequestClientFormField>,
	action: Action<UseFormAsyncSubmitOutcome<AuthResponse>, ServerFnError>,
	username: Signal<String>,
	password: Signal<String>,
	password_input: Callback<InputEvent, ()>,
}

fn login_field_error(
	state: FormState<LoginRequestClientFormField>,
	field: LoginRequestClientFormField,
) -> Page {
	Page::reactive(move || {
		state
			.field_errors
			.get()
			.get(&field)
			.map(|error| {
				let message = error.message().to_owned();
				page!({
					p {
						class: "mt-1 text-xs font-medium text-red-700",
						role: "alert",
						{ message }
					}
				})
			})
			.unwrap_or(Page::Empty)
	})
}

fn login_form_error(state: FormState<LoginRequestClientFormField>) -> Page {
	Page::reactive(move || {
		state
			.form_error
			.get()
			.or_else(|| state.submit_error.get())
			.map(|message| {
				page!({
					div {
						class: "rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm font-medium text-red-700",
						role: "alert",
						{ message }
					}
				})
			})
			.unwrap_or(Page::Empty)
	})
}

fn render_login_form(view: LoginFormView) -> Page {
	let LoginFormView {
		state,
		action,
		username,
		password,
		password_input,
	} = view;
	let submit = Callback::new(move |event: SubmitEvent| {
		event.prevent_default();
		action.dispatch(());
	});
	let form_error = login_form_error(state.clone());
	let username_error = login_field_error(state.clone(), LoginRequestClientFormField::Username);
	let password_error = login_field_error(state.clone(), LoginRequestClientFormField::Password);

	Page::reactive(move || {
		let is_submitting = state.is_submitting.get();
		let password_value = password.get();
		let submit_label = if is_submitting {
			"Signing in..."
		} else {
			"Sign in"
		};
		page!({
			form {
				class: "rc-form-stack",
				@submit: submit,
				{ form_error }
				div {
					class: "rc-field",
					label {
						class: "rc-label",
						r#for: "login-username",
						"Username"
					}
					input {
						id: "login-username",
						aria_label: "Username",
						aria_describedby: "login-username-error",
						class: "rc-input",
						type: "text",
						autocomplete: "username",
						maxlength: 150,
						placeholder: "Enter your username",
						bind: username,
					}
					div {
						id: "login-username-error",
						{ username_error }
					}
				}
				div {
					class: "rc-field",
					label {
						class: "rc-label",
						r#for: "login-password",
						"Password"
					}
					input {
						id: "login-password",
						aria_label: "Password",
						aria_describedby: "login-password-error",
						class: "rc-input",
						type: "password",
						autocomplete: "current-password",
						maxlength: 128,
						placeholder: "Enter your password",
						value: password_value,
						@input: password_input,
					}
					div {
						id: "login-password-error",
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
		})
	})
}

fn evict_authenticated_query_families(query_client: &QueryClient) {
	query_client.remove_family(me::family());
	query_client.remove_family(list_oauth_providers::family());
	query_client.remove_family(list_linked_oauth_accounts::family());
	query_client.remove_family(list_clusters_for_current_org::family());
	query_client.remove_family(list_deployments_for_current_org::family());
	query_client.remove_family(list_deployment_previews_for_current_org::family());
	query_client.remove_family(deployment_logs_for_current_org::family());
	query_client.remove_family(get_github_onboarding_for_current_org::family());
	query_client.remove_family(list_github_repositories_for_current_org::family());
	query_client.remove_family(list_github_repositories_for_installation::family());
	query_client.remove_family(list_github_project_previews_for_current_org::family());
}

#[cfg(wasm)]
fn replace_document(location: &str) -> Result<(), ServerFnError> {
	let window = web_sys::window()
		.ok_or_else(|| ServerFnError::server(500, "Browser window is unavailable"))?;
	window
		.location()
		.replace(location)
		.map_err(|error| ServerFnError::server(500, format!("Unable to finish sign in: {error:?}")))
}

#[cfg(not(wasm))]
fn replace_document(_location: &str) -> Result<(), ServerFnError> {
	Ok(())
}

/// Render the login page.
#[reinhardt::pages::component("/login", name = "auth:login_page")]
pub fn login_page() -> Page {
	let login_form = LoginRequestClientForm::new();
	let query_client = queries();
	let home_href = route_href("dashboard:home", "/");
	let submit_query_client = query_client.clone();
	let submit_home_href = home_href.clone();
	let login_runtime = use_form(&login_form)
		.on_submit_success(move |runtime| {
			evict_authenticated_query_families(&submit_query_client);
			if let Err(error) = replace_document(&submit_home_href) {
				runtime.apply_server_error(&ServerFnError::application(format!(
					"Signed in, but navigation to the dashboard failed: {error}"
				)));
			}
		})
		.build();
	let login_state = login_runtime.form_state();
	let username = login_runtime.watch_field::<String>(login_form.username_field());
	let password = login_runtime.watch_field::<String>(login_form.password_field());
	let password_input = {
		Callback::new(move |event: InputEvent| {
			let Ok(value) = event.value() else {
				return;
			};
			password.set(value);
		})
	};
	let submit_form = login_form.clone();
	let submit_runtime = login_runtime.clone();
	let login_action = use_action(move |(): ()| {
		let form = submit_form.clone();
		let runtime = submit_runtime.clone();
		async move {
			#[cfg(wasm)]
			{
				form.submit(&runtime).await
			}
			#[cfg(not(wasm))]
			{
				let _ = (form, runtime);
				Ok(UseFormAsyncSubmitOutcome::ValidationFailed)
			}
		}
	});
	let form_view = render_login_form(LoginFormView {
		state: login_state,
		action: login_action,
		username,
		password,
		password_input,
	});
	let oauth_buttons = oauth_buttons();
	let register_href = route_href("auth:register_page", "/register");
	page!({
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
						"Sign in to your account"
					}
					{ form_view }
					{ oauth_buttons }
					div {
						class: "mt-6 text-center text-sm text-ink-600",
						"Don't have an account? " a {
							href: register_href,
							class: "font-semibold text-control-700 underline-offset-4 hover:underline",
							"Create one"
						}
					}
				}
			}
		}
	})
}

#[cfg(test)]
mod tests {
	#[cfg(native)]
	use std::cell::{Cell, RefCell};
	#[cfg(native)]
	use std::rc::Rc;

	#[cfg(native)]
	use reinhardt::pages::prelude::{QueryHandle, QueryOptions, QueryStatus, use_query};
	use reinhardt::pages::reactive::ReactiveScope;
	#[cfg(native)]
	use reinhardt::pages::testing::component::render;
	use rstest::rstest;

	#[cfg(native)]
	use crate::shared::UserInfo;

	use super::*;

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
		captured: Rc<RefCell<Option<(QueryClient, QueryHandle<UserInfo, ServerFnError>)>>>,
	) -> Page {
		let query_client = queries();
		let query = use_query(
			me::family().query((), move || {
				fetches.set(fetches.get() + 1);
				let user = user.clone();
				async move { Ok::<UserInfo, ServerFnError>(user) }
			}),
			QueryOptions::new(),
		);
		*captured.borrow_mut() = Some((query_client, query.clone()));
		Page::reactive(move || {
			query
				.data()
				.map(|user| Page::text(user.username))
				.unwrap_or_else(|| Page::text("Loading"))
		})
	}

	#[cfg(native)]
	#[rstest]
	#[tokio::test]
	async fn login_evicts_authenticated_query_cache_before_document_reload() {
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
		let (query_client, query) = captured
			.borrow_mut()
			.take()
			.expect("the mounted current-user query should be captured");

		// Act
		evict_authenticated_query_families(&query_client);

		// Assert
		assert_eq!(first_document.pretty(), "alice\n");
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
