//! Clusters list and CRUD page.

use std::collections::HashMap;
use std::hash::Hash;

use reinhardt::pages::component;
use reinhardt::pages::component::Page;
use reinhardt::pages::event::{ClickEvent, InputEvent, SubmitEvent};
use reinhardt::pages::form;
use reinhardt::pages::page;
use reinhardt::pages::prelude::{
	Action, Callback, FieldError, FormState, QueryClient, QueryHandle, QueryOptions, QueryStatus,
	Signal, UseFormAsyncSubmitOutcome, queries, use_action, use_callback, use_form, use_query,
};
use reinhardt::pages::reactive::ExplicitDeps;
use reinhardt::pages::server_fn::ServerFnError;

use crate::apps::clusters::model_form::{
	ClusterCreateFields, ClusterCreateFormFormSchema, ClusterCreateFormModelFormData,
};
use crate::apps::clusters::server_fn::{
	ClusterInfo, ClusterTokenInfo, UpdateClusterFormRequest, UpdateClusterFormRequestClientForm,
	UpdateClusterFormRequestClientFormField, create_cluster_for_current_org,
	list_clusters_for_current_org,
};
#[cfg(wasm)]
use crate::apps::clusters::server_fn::{
	delete_cluster_for_current_org, rotate_cluster_token_for_current_org,
	update_cluster_for_current_org,
};
use crate::apps::deployments::client::components::cluster_health::cluster_health_container;
use crate::shared::client::components::entity_select::{EntitySelectOption, entity_select};

fn alert(error: Signal<Option<String>>) -> Page {
	page!({
		{
			error
	.get()
	.map(|message| {
		page!({
			div {
				class: "rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm font-medium text-red-700",
				{ message }
			}
		})
	})
	.unwrap_or(Page::Empty)
		}
	})
}

fn query_error_message(error: Option<ServerFnError>, fallback: &'static str) -> String {
	error
		.map(|error| error.user_message().to_owned())
		.unwrap_or_else(|| fallback.to_owned())
}

fn query_refetch_notice(
	is_fetching: bool,
	refetch_error: Option<ServerFnError>,
	label: &'static str,
) -> Page {
	if let Some(error) = refetch_error {
		let message = format!(
			"Showing cached {label}; the latest refresh failed: {}",
			error.user_message()
		);
		return page!({
			div {
				class: "border-b border-amber-100 bg-amber-50 px-4 py-2 text-xs font-medium text-amber-700",
				{ message }
			}
		});
	}
	if is_fetching {
		return page!({
			div {
				class: "border-b border-cloud-100 bg-cloud-50 px-4 py-2 text-xs font-medium text-cloud-600",
				"Refreshing " { label }"..."
			}
		});
	}
	Page::Empty
}

fn invalidate_cluster_list_query(query_client: &QueryClient) {
	query_client.invalidate(&list_clusters_for_current_org::key());
}

fn invalidate_cluster_query_family(query_client: &QueryClient) {
	query_client.invalidate_family(list_clusters_for_current_org::family());
}

#[cfg(wasm)]
fn clear_create_cluster_inputs() {
	use wasm_bindgen::JsCast;

	let Some(document) = web_sys::window().and_then(|window| window.document()) else {
		return;
	};
	for id in ["create-cluster-name", "create-cluster-api-url"] {
		if let Some(input) = document
			.get_element_by_id(id)
			.and_then(|element| element.dyn_into::<web_sys::HtmlInputElement>().ok())
		{
			input.set_value("");
		}
	}
}

#[cfg(not(wasm))]
fn clear_create_cluster_inputs() {}

#[cfg(wasm)]
async fn submit_cluster_update(
	request: UpdateClusterFormRequest,
) -> Result<ClusterInfo, ServerFnError> {
	update_cluster_for_current_org(request).await
}

#[cfg(not(wasm))]
fn unavailable_during_server_rendering<T>(operation: &str) -> Result<T, ServerFnError> {
	Err(ServerFnError::application(format!(
		"Cluster {operation} is unavailable during server rendering"
	)))
}

#[cfg(not(wasm))]
async fn submit_cluster_update(
	_request: UpdateClusterFormRequest,
) -> Result<ClusterInfo, ServerFnError> {
	unavailable_during_server_rendering("updates")
}

#[cfg(wasm)]
async fn submit_cluster_delete(cluster_id: String) -> Result<(), ServerFnError> {
	delete_cluster_for_current_org(cluster_id).await
}

#[cfg(not(wasm))]
async fn submit_cluster_delete(_cluster_id: String) -> Result<(), ServerFnError> {
	unavailable_during_server_rendering("deletion")
}

#[cfg(wasm)]
async fn submit_cluster_token_rotation(
	cluster_id: String,
) -> Result<ClusterTokenInfo, ServerFnError> {
	rotate_cluster_token_for_current_org(cluster_id).await
}

#[cfg(not(wasm))]
async fn submit_cluster_token_rotation(
	_cluster_id: String,
) -> Result<ClusterTokenInfo, ServerFnError> {
	unavailable_during_server_rendering("token rotation")
}

fn cluster_token_confirmation_with_callback(
	token: ClusterTokenInfo,
	dismiss: Callback<ClickEvent, ()>,
) -> Page {
	page!({
		div {
			class: "mt-3 rounded-md border border-amber-300 bg-amber-50 p-3 text-sm text-amber-950",
			p {
				class: "font-semibold",
				{ format!("{} is ready.", token.cluster.name) }
			}
			p {
				class: "mt-1",
				"Save this agent token now. It cannot be shown again."
			}
			code {
				class: "mt-2 block break-all rounded bg-white px-2 py-1 font-mono text-xs",
				{ token.auth_token }
			}
			button {
				type: "button",
				class: "btn-dark mt-3 min-h-10",
				@click: dismiss,
				"I have saved this token"
			}
		}
	})
}

fn success_alert(message: Signal<Option<String>>) -> Page {
	page!({
		{
			message
	.get()
	.map(|message| {
		page!({
			div {
				class: "rounded-md border border-green-200 bg-green-50 px-3 py-2 text-sm font-medium text-green-800",
				{ message }
			}
		})
	})
	.unwrap_or(Page::Empty)
		}
	})
}

fn form_field_error<Field>(field_errors: Signal<HashMap<Field, FieldError>>, field: Field) -> Page
where
	Field: Copy + Eq + Hash + 'static,
{
	page!({
		{
			field_errors
				.get()
				.get(&field)
				.map(|error| {
					let message = error.message().to_owned();
					page!({
						p {
							class: "mt-1 text-xs font-medium text-red-700",
							{ message }
						}
					})
				})
				.unwrap_or(Page::Empty)
		}
	})
}

#[derive(Clone)]
struct ClusterUpdateFormView {
	state: FormState<UpdateClusterFormRequestClientFormField>,
	action: Action<UseFormAsyncSubmitOutcome<ClusterInfo>, ServerFnError>,
	success: Signal<Option<String>>,
	name: Signal<String>,
	api_url: Signal<String>,
	is_active: Signal<bool>,
}

fn render_cluster_update_form(view: ClusterUpdateFormView) -> Page {
	let ClusterUpdateFormView {
		state,
		action,
		success,
		name,
		api_url,
		is_active,
	} = view;
	let submit = Callback::new(move |event: SubmitEvent| {
		event.prevent_default();
		action.dispatch(());
	});
	let success_view = success_alert(success);
	let error_view = alert(state.form_error);
	let name_error = form_field_error(
		state.field_errors,
		UpdateClusterFormRequestClientFormField::Name,
	);
	let api_url_error = form_field_error(
		state.field_errors,
		UpdateClusterFormRequestClientFormField::ApiUrl,
	);

	page!({
		{
			let is_submitting = state.is_submitting.get();
			let dirty_notice = if state.is_dirty.get() {
				page!({
					p {
						class: "mt-2 text-xs text-amber-700",
						"Unsaved changes"
					}
				})
			} else {
				Page::Empty
			};
			let submit_status = if is_submitting {
				page!({
					p {
						class: "mt-2 text-xs text-ink-600",
						"Updating..."
					}
				})
			} else {
				Page::Empty
			};
			page!({
				{ success_view }
				{ error_view }
				form {
					class: "rc-form-stack mt-3",
					@submit: submit,
					div {
						class: "rc-field",
						label {
							span { class: "rc-label", "Name" }
							input {
								id: "update-cluster-name",
								aria_label: "Cluster name",
								class: "rc-input",
								type: "text",
								maxlength: 63,
								bind: name,
							}
						}
						{ name_error }
					}
					div {
						class: "rc-field",
						label {
							span { class: "rc-label", "API URL" }
							input {
								id: "update-cluster-api-url",
								aria_label: "Cluster API URL",
								class: "rc-input",
								type: "text",
								maxlength: 2048,
								bind: api_url,
							}
						}
						{ api_url_error }
					}
					label {
						class: "rc-checkbox-field",
						input {
							id: "update-cluster-active",
							type: "checkbox",
							bind: is_active,
						}
						span { "Active" }
					}
					button {
						type: "submit",
						class: "btn-dark min-h-11 w-full",
						disabled: is_submitting,
						"Update cluster"
					}
				}
				{ dirty_notice }
				{ submit_status }
			})
		}
	})
}

#[derive(Clone)]
struct DeleteClusterActionView {
	action: Action<(), ServerFnError>,
	cluster_id: Signal<String>,
	confirmed: Signal<bool>,
	error: Signal<Option<String>>,
	success: Signal<Option<String>>,
}

fn render_delete_cluster_action(view: DeleteClusterActionView) -> Page {
	let DeleteClusterActionView {
		action,
		cluster_id,
		confirmed,
		error,
		success,
	} = view;
	let delete = Callback::new(move |event: ClickEvent| {
		event.prevent_default();
		action.dispatch(cluster_id.get());
	});
	let error_view = alert(error);
	let success_view = success_alert(success);
	page!({
		{
			let is_pending = action.is_pending();
			let is_confirmed = confirmed.get();
			let has_selected_cluster = !cluster_id.get().trim().is_empty();
			page!({
				{ success_view }
				{ error_view }
				div {
					class: "rc-form-stack mt-3",
					label {
						class: "flex items-start gap-2 text-sm text-ink-700",
						input {
							id: "confirm-cluster-delete",
							type: "checkbox",
							bind: confirmed,
						}
						span { "I understand this permanently deletes the selected cluster." }
					}
					button {
						type: "button",
						class: "btn-danger min-h-11 w-full",
						disabled: !is_confirmed || !has_selected_cluster || is_pending,
						@click: delete,
						"Delete cluster"
					} {
						if is_pending {
							page!( {
								p {
									class: "text-xs text-ink-600",
									"Deleting..."
								}
							})
						} else { Page::Empty }
					}
				}
			})
		}
	})
}

#[derive(Clone)]
struct RotateClusterTokenActionView {
	action: Action<ClusterTokenInfo, ServerFnError>,
	cluster_id: Signal<String>,
	confirmed: Signal<bool>,
	error: Signal<Option<String>>,
}

fn render_rotate_cluster_token_action(view: RotateClusterTokenActionView) -> Page {
	let RotateClusterTokenActionView {
		action,
		cluster_id,
		confirmed,
		error,
	} = view;
	let rotate = Callback::new(move |event: ClickEvent| {
		event.prevent_default();
		if action.is_pending() {
			return;
		}
		action.reset();
		action.dispatch(cluster_id.get());
	});
	let dismiss = Callback::new(move |event: ClickEvent| {
		event.prevent_default();
		action.reset();
	});
	let error_view = alert(error);
	page!({
		{
			let is_pending = action.is_pending();
			let is_confirmed = confirmed.get();
			let has_selected_cluster = !cluster_id.get().trim().is_empty();
			let token_confirmation = action
				.result()
				.map(|token| self::cluster_token_confirmation_with_callback(token, dismiss));
			let token_confirmation = token_confirmation.unwrap_or(Page::Empty);
			page!({
				{ error_view }
				div {
					class: "rc-form-stack mt-3",
					label {
						class: "flex items-start gap-2 text-sm text-ink-700",
						input {
							id: "confirm-cluster-token-rotation",
							type: "checkbox",
							bind: confirmed,
						}
						span { "I understand this invalidates the current agent token." }
					}
					button {
						type: "button",
						class: "btn-warning min-h-11 w-full",
						disabled: !is_confirmed || !has_selected_cluster || is_pending,
						@click: rotate,
						"Rotate token"
					} {
						if is_pending {
							page!( {
								p {
									class: "text-xs text-ink-600",
									"Rotating..."
								}
							})
						} else { Page::Empty }
					}
				}
				{ token_confirmation }
			})
		}
	})
}

fn cluster_select_options(items: &[ClusterInfo]) -> Vec<EntitySelectOption> {
	items
		.iter()
		.map(|cluster| {
			let state = if cluster.is_active {
				"active"
			} else {
				"inactive"
			};
			EntitySelectOption::new(
				cluster.id.to_string(),
				cluster.name.clone(),
				Some(format!("{state} / {}", cluster.api_url)),
			)
		})
		.collect()
}

fn render_cluster_inventory(items: Vec<ClusterInfo>) -> Page {
	if items.is_empty() {
		return page!({
			div {
				class: "rc-empty",
				"No clusters registered."
			}
		});
	}

	page!({
		div {
			class: "overflow-x-auto",
			table {
				class: "rc-table",
				thead {
					class: "bg-cloud-50",
					tr {
						th {
							class: "rc-th",
							"ID"
						}
						th {
							class: "rc-th",
							"Name"
						}
						th {
							class: "rc-th",
							"API URL"
						}
						th {
							class: "rc-th",
							"Active"
						}
						th {
							class: "rc-th",
							"Token Rotated"
						}
					}
				}
				tbody {
					class: "divide-y divide-cloud-100 bg-white",
					{ items.clone().into_iter().map(|cluster| page!({
						tr {
							td {
								class: "px-4 py-2 font-mono text-xs text-ink-600",
								{ cluster.id.to_string() }
							}
							td {
								class: "px-4 py-2 font-semibold text-ink-950",
								{ cluster.name }
							}
							td {
								class: "px-4 py-2 text-ink-600",
								{ cluster.api_url }
							}
							td {
								class: "px-4 py-2",
								span {
									class: if cluster.is_active { "rounded-full bg-control-500/10 px-2 py-0.5 text-xs font-semibold text-control-700" } else { "rounded-full bg-cloud-100 px-2 py-0.5 text-xs font-semibold text-ink-600" },
									{ if cluster.is_active { "Active" } else { "Inactive" } }
								}
							}
							td {
								class: "px-4 py-2 text-ink-600",
								{ cluster.token_last_rotated_at.clone().unwrap_or_else(|| "never".to_string()) }
							}
						}
					})).collect::<Vec<_>>() }
				}
			}
		}
	})
}

#[derive(Clone)]
struct ClustersListPageViewProps {
	clusters_for_inventory: QueryHandle<Vec<ClusterInfo>, ServerFnError>,
	clusters_for_edit: QueryHandle<Vec<ClusterInfo>, ServerFnError>,
	clusters_for_rotate: QueryHandle<Vec<ClusterInfo>, ServerFnError>,
	clusters_for_delete: QueryHandle<Vec<ClusterInfo>, ServerFnError>,
	create_view: Page,
	edit_view: Page,
	edit_cluster_id: Signal<String>,
	edit_selection_changed: Callback<UpdateClusterFormRequest, ()>,
	delete_view: Page,
	delete_cluster_id: Signal<String>,
	delete_selection_changed: Callback<String, ()>,
	rotate_view: Page,
	rotate_cluster_id: Signal<String>,
	rotate_selection_changed: Callback<String, ()>,
	health: Page,
}

/// Render the clusters page.
#[component("clusters", name = "clusters:list")]
pub fn clusters_list_page() -> Page {
	let clusters = use_query(
		list_clusters_for_current_org::query(),
		QueryOptions::new().enabled(cfg!(wasm)),
	);
	let query_client = queries();

	let create_form = form! {
		name: CreateClusterForm,
		model: ClusterCreateForm,
		policy: ClusterCreateFields,
		fields: [name, api_url],
		server_fn: create_cluster_for_current_org,
		class: "rc-form-grid",
		overrides: {
			name: {
				label: "Name",
				help_text: "For example: prod-us-east",
			}
			api_url: {
				label: "API URL",
				help_text: "For example: https://kubernetes.example.com:6443",
			}
		}
	};
	let create_query_client = query_client.clone();
	let create_runtime = use_form(&create_form)
		.on_submit_success(move |runtime| {
			self::invalidate_cluster_list_query(&create_query_client);
			runtime.reset();
			self::clear_create_cluster_inputs();
		})
		.build();
	let create_state = create_runtime.form_state();
	let create_name_form = create_form.clone();
	let create_name_input = use_callback(
		move |event: InputEvent| {
			let Ok(value) = event.value() else {
				return;
			};
			let _ = create_name_form.set_value("name", serde_json::Value::String(value));
		},
		ExplicitDeps::from_node_ids([]),
	);
	let create_api_url_form = create_form.clone();
	let create_api_url_input = use_callback(
		move |event: InputEvent| {
			let Ok(value) = event.value() else {
				return;
			};
			let _ = create_api_url_form.set_value("api_url", serde_json::Value::String(value));
		},
		ExplicitDeps::from_node_ids([]),
	);
	// ModelForm responses are client-only in alpha.11 because native SSR does
	// not dispatch the mutation and cannot receive the one-time token payload.
	#[cfg(wasm)]
	let create_action = {
		let runtime = create_runtime.clone();
		let form = create_form.clone();
		use_action(move |(): ()| {
			let runtime = runtime.clone();
			let form = form.clone();
			async move { runtime.submit_server_fn(|| form.submit_response()).await }
		})
	};
	#[cfg(not(wasm))]
	let create_action = use_action(|(): ()| async {
		Err::<UseFormAsyncSubmitOutcome<ClusterTokenInfo>, ServerFnError>(
			ServerFnError::application(
				"Cluster registration is unavailable during server rendering",
			),
		)
	});
	let create_submit = Callback::new(move |event: SubmitEvent| {
		event.prevent_default();
		if !create_action.is_pending() {
			create_action.dispatch(());
		}
	});
	let create_dismiss = Callback::new(move |event: ClickEvent| {
		event.prevent_default();
		create_action.reset();
	});
	let create_error = alert(create_state.form_error);
	let create_name_error = form_field_error(create_state.field_errors, create_form.name_field());
	let create_api_url_error =
		form_field_error(create_state.field_errors, create_form.api_url_field());
	let create_view = page!({
		{
			let is_submitting = create_action.is_pending();
			let token_confirmation = create_action
				.result()
				.and_then(|outcome| match outcome {
					UseFormAsyncSubmitOutcome::Submitted(token) => Some(token),
					UseFormAsyncSubmitOutcome::AlreadyPending
					| UseFormAsyncSubmitOutcome::ValidationFailed => None,
				})
				.map(|token| self::cluster_token_confirmation_with_callback(token, create_dismiss));
			let token_confirmation = token_confirmation.unwrap_or(Page::Empty);
			page!({
				div {
					class: "space-y-3",
					{ create_error }
					form {
						class: "rc-form-grid",
						@submit: create_submit,
						div {
							class: "rc-field",
							label {
								span { class: "rc-label", "Name" }
								input {
									id: "create-cluster-name",
									name: "name",
									aria_label: "Cluster name",
									class: "rc-input",
									type: "text",
									maxlength: 63,
									placeholder: "prod-us-east",
									@input: create_name_input,
								}
							}
							p {
								class: "mt-1 text-xs text-ink-600",
								"For example: prod-us-east"
							}
							{ create_name_error }
						}
						div {
							class: "rc-field",
							label {
								span { class: "rc-label", "API URL" }
								input {
									id: "create-cluster-api-url",
									name: "api_url",
									aria_label: "Cluster API URL",
									class: "rc-input",
									type: "text",
									maxlength: 2048,
									placeholder: "https://kubernetes.example.com:6443",
									@input: create_api_url_input,
								}
							}
							p {
								class: "mt-1 text-xs text-ink-600",
								"For example: https://kubernetes.example.com:6443"
							}
							{ create_api_url_error }
						}
						button {
							type: "submit",
							class: "btn-primary min-h-11 w-full md:w-auto md:justify-self-start",
							disabled: is_submitting,
							{
								if is_submitting { "Registering..." } else { "Register cluster" }
							}
						}
					}
					{ token_confirmation }
				}
			})
		}
	});

	let edit_form =
		UpdateClusterFormRequestClientForm::new().with_defaults(UpdateClusterFormRequest {
			cluster_id: String::new(),
			name: String::new(),
			api_url: String::new(),
			is_active: true,
		});
	let edit_success = Signal::new(None::<String>);
	let edit_query_client = query_client.clone();
	let edit_success_callback = edit_success;
	let edit_runtime = use_form(&edit_form)
		.on_submit_success(move |runtime| {
			self::invalidate_cluster_list_query(&edit_query_client);
			runtime.reset();
			edit_success_callback.set(Some("Cluster updated.".to_owned()));
		})
		.build();
	let edit_state = edit_runtime.form_state();
	let edit_cluster_id = edit_runtime.watch_field::<String>(edit_form.cluster_id_field());
	let edit_name = edit_runtime.watch_field::<String>(edit_form.name_field());
	let edit_api_url = edit_runtime.watch_field::<String>(edit_form.api_url_field());
	let edit_is_active = edit_runtime.watch_field::<bool>(edit_form.is_active_field());
	let edit_action_runtime = edit_runtime.clone();
	let edit_action = use_action(move |(): ()| {
		let runtime = edit_action_runtime.clone();
		async move {
			let request = UpdateClusterFormRequestClientForm::to_request(&runtime);
			runtime
				.submit_server_fn(|| async move { submit_cluster_update(request).await })
				.await
		}
	});
	let edit_runtime_for_selection = edit_runtime.clone();
	let edit_success_for_selection = edit_success;
	let edit_selection_changed = Callback::new(move |request: UpdateClusterFormRequest| {
		edit_runtime_for_selection.set_value(
			UpdateClusterFormRequestClientFormField::ClusterId,
			request.cluster_id,
		);
		edit_runtime_for_selection
			.set_value(UpdateClusterFormRequestClientFormField::Name, request.name);
		edit_runtime_for_selection.set_value(
			UpdateClusterFormRequestClientFormField::ApiUrl,
			request.api_url,
		);
		edit_runtime_for_selection.set_value(
			UpdateClusterFormRequestClientFormField::IsActive,
			request.is_active,
		);
		edit_runtime_for_selection.reset_default_values();
		edit_runtime_for_selection.clear_errors();
		edit_success_for_selection.set(None);
	});
	let edit_view = self::render_cluster_update_form(ClusterUpdateFormView {
		state: edit_state,
		action: edit_action,
		success: edit_success,
		name: edit_name,
		api_url: edit_api_url,
		is_active: edit_is_active,
	});

	let delete_cluster_id = Signal::new(String::new());
	let delete_confirmed = Signal::new(false);
	let delete_error = Signal::new(None::<String>);
	let delete_success = Signal::new(None::<String>);
	let delete_query_client = query_client.clone();
	let delete_confirmed_for_action = delete_confirmed;
	let delete_error_for_action = delete_error;
	let delete_cluster_id_for_success = delete_cluster_id;
	let delete_confirmed_for_success = delete_confirmed;
	let delete_success_for_success = delete_success;
	let delete_error_for_callback = delete_error;
	let delete_action = use_action(move |cluster_id: String| {
		delete_error_for_action.set(None);
		let confirmed = delete_confirmed_for_action.get();
		async move {
			if !confirmed {
				return Err(ServerFnError::application(
					"Confirm deletion before continuing",
				));
			}
			if cluster_id.trim().is_empty() {
				return Err(ServerFnError::application(
					"Select a cluster before deleting",
				));
			}
			submit_cluster_delete(cluster_id).await
		}
	})
	.on_success(move |_| {
		self::invalidate_cluster_query_family(&delete_query_client);
		delete_cluster_id_for_success.set(String::new());
		delete_confirmed_for_success.set(false);
		delete_success_for_success.set(Some("Cluster deleted.".to_owned()));
	})
	.on_error(move |error| {
		delete_error_for_callback.set(Some(error.user_message().to_owned()));
	});
	let delete_confirmed_for_selection = delete_confirmed;
	let delete_error_for_selection = delete_error;
	let delete_success_for_selection = delete_success;
	let delete_action_for_selection = delete_action;
	let delete_selection_changed = Callback::new(move |_cluster_id: String| {
		delete_confirmed_for_selection.set(false);
		delete_error_for_selection.set(None);
		delete_success_for_selection.set(None);
		delete_action_for_selection.reset();
	});
	let delete_view = self::render_delete_cluster_action(DeleteClusterActionView {
		action: delete_action,
		cluster_id: delete_cluster_id,
		confirmed: delete_confirmed,
		error: delete_error,
		success: delete_success,
	});

	let rotate_cluster_id = Signal::new(String::new());
	let rotate_confirmed = Signal::new(false);
	let rotate_error = Signal::new(None::<String>);
	let rotate_query_client = query_client.clone();
	let rotate_confirmed_for_action = rotate_confirmed;
	let rotate_error_for_action = rotate_error;
	let rotate_confirmed_for_success = rotate_confirmed;
	let rotate_error_for_callback = rotate_error;
	let rotate_action = use_action(move |cluster_id: String| {
		rotate_error_for_action.set(None);
		let confirmed = rotate_confirmed_for_action.get();
		async move {
			if !confirmed {
				return Err(ServerFnError::application(
					"Confirm token rotation before continuing",
				));
			}
			if cluster_id.trim().is_empty() {
				return Err(ServerFnError::application(
					"Select a cluster before rotating its token",
				));
			}
			submit_cluster_token_rotation(cluster_id).await
		}
	})
	.on_success(move |_| {
		self::invalidate_cluster_query_family(&rotate_query_client);
		rotate_confirmed_for_success.set(false);
	})
	.on_error(move |error| {
		rotate_error_for_callback.set(Some(error.user_message().to_owned()));
	});
	let rotate_confirmed_for_selection = rotate_confirmed;
	let rotate_error_for_selection = rotate_error;
	let rotate_action_for_selection = rotate_action;
	let rotate_selection_changed = Callback::new(move |_cluster_id: String| {
		rotate_confirmed_for_selection.set(false);
		rotate_error_for_selection.set(None);
		rotate_action_for_selection.reset();
	});
	let rotate_view = self::render_rotate_cluster_token_action(RotateClusterTokenActionView {
		action: rotate_action,
		cluster_id: rotate_cluster_id,
		confirmed: rotate_confirmed,
		error: rotate_error,
	});

	let health = cluster_health_container();
	let clusters_for_inventory = clusters.clone();
	let clusters_for_edit = clusters.clone();
	let clusters_for_rotate = clusters.clone();
	let clusters_for_delete = clusters.clone();

	let props = ClustersListPageViewProps {
		clusters_for_inventory,
		clusters_for_edit,
		clusters_for_rotate,
		clusters_for_delete,
		create_view,
		edit_view,
		edit_cluster_id,
		edit_selection_changed,
		delete_view,
		delete_cluster_id,
		delete_selection_changed,
		rotate_view,
		rotate_cluster_id,
		rotate_selection_changed,
		health,
	};

	page!({
		div {
			class: "rc-shell",
			div {
				class: "space-y-0",
				div {
					class: "rc-topline",
					div {
						p {
							class: "rc-kicker",
							"Infrastructure"
						}
						h1 {
							class: "rc-title",
							"Clusters"
						}
						p {
							class: "rc-muted mt-1",
							"Registered Kubernetes clusters and agent health."
						}
					}
				}
				div {
					class: "grid gap-6 lg:grid-cols-[1fr_320px]",
					div {
						class: "space-y-6",
						section {
							class: "rc-panel",
							div {
								class: "rc-panel-head",
								"Cluster Inventory"
							}{
								let snapshot = props.clusters_for_inventory.snapshot();
								match snapshot.status {
									QueryStatus::Idle => page!({
										div {
											class: "rc-empty",
											"Clusters are not available during server rendering."
										}
									}),
									QueryStatus::Pending => page!({
										div {
											class: "rc-empty",
											"Loading clusters..."
										}
									}),
									QueryStatus::Error => {
										let message = self::query_error_message(
											snapshot.error,
											"Clusters are temporarily unavailable.",
										);
										page!({
											div {
												class: "px-4 py-8 text-sm font-medium text-red-700",
												{ message }
											}
										})
									}
									QueryStatus::Success => {
										let notice = self::query_refetch_notice(
											snapshot.is_fetching,
											snapshot.refetch_error,
											"clusters",
										);
										let inventory =
											self::render_cluster_inventory(snapshot.data.unwrap_or_default());
										page!({
											{ notice }
											{ inventory }
										})
									}
								}
							}
						}
						section {
							class: "rc-panel-pad",
							h2 {
								class: "mb-3 text-sm font-semibold text-ink-950",
								"Register Cluster"
							}
							{ props.create_view.clone() }
						}
						section {
							class: "rc-panel-pad",
							h2 {
								class: "mb-3 text-sm font-semibold text-ink-950",
								"Agent Health"
							}
							{ props.health.clone() }
						}
					}
					aside {
						class: "rc-stack",
						section {
							class: "rc-panel-pad",
							h2 {
								class: "mb-3 text-sm font-semibold text-ink-950",
								"Cluster Operations"
							}
							{
								let snapshot = props.clusters_for_edit.snapshot();
								match snapshot.status {
									QueryStatus::Success => {
										let items = snapshot.data.unwrap_or_default();
										let clusters_for_change = items.clone();
										let selection_changed = props.edit_selection_changed;
										let selector = self::entity_select(
											"Cluster",
											"Select cluster",
											self::cluster_select_options(&items),
											props.edit_cluster_id,
											move |value| {
												if let Some(cluster) = clusters_for_change
													.iter()
													.find(|cluster| cluster.id.to_string() == value)
												{
													selection_changed.call(UpdateClusterFormRequest {
														cluster_id: value,
														name: cluster.name.clone(),
														api_url: cluster.api_url.clone(),
														is_active: cluster.is_active,
													});
												}
											},
										);
										let notice = self::query_refetch_notice(
											snapshot.is_fetching,
											snapshot.refetch_error,
											"clusters",
										);
										page!({
											{ notice }
											{ selector }
										})
									}
									QueryStatus::Idle => page!({
										p {
											class: "mb-3 text-xs text-cloud-500",
											"Clusters are not available during server rendering."
										}
									}),
									QueryStatus::Pending => page!({
										p {
											class: "mb-3 text-xs text-ink-600",
											"Loading clusters..."
										}
									}),
									QueryStatus::Error => {
										let message = self::query_error_message(
											snapshot.error,
											"Clusters are temporarily unavailable.",
										);
										page!({
											p {
												class: "mb-3 text-xs font-medium text-red-700",
												{ message }
											}
										})
									}
								}
							} {
								props.edit_view.clone()
							}
							div {
								class: "my-4 border-t border-cloud-200"
							}
							{
								let snapshot = props.clusters_for_rotate.snapshot();
								match snapshot.status {
									QueryStatus::Success => {
										let items = snapshot.data.unwrap_or_default();
										let selection_changed = props.rotate_selection_changed;
										let notice = self::query_refetch_notice(
											snapshot.is_fetching,
											snapshot.refetch_error,
											"clusters",
										);
										let selector = self::entity_select(
											"Cluster",
											"Select cluster",
											self::cluster_select_options(&items),
											props.rotate_cluster_id,
											move |value| selection_changed.call(value),
										);
										page!({
											{ notice }
											{ selector }
										})
									}
									QueryStatus::Idle => page!({
										p {
											class: "mb-3 text-xs text-cloud-500",
											"Clusters are not available during server rendering."
										}
									}),
									QueryStatus::Pending => page!({
										p {
											class: "mb-3 text-xs text-ink-600",
											"Loading clusters..."
										}
									}),
									QueryStatus::Error => {
										let message = self::query_error_message(
											snapshot.error,
											"Clusters are temporarily unavailable.",
										);
										page!({
											p {
												class: "mb-3 text-xs font-medium text-red-700",
												{ message }
											}
										})
									}
								}
							} {
								props.rotate_view.clone()
							}
							div {
								class: "my-4 border-t border-cloud-200"
							}
							{
								let snapshot = props.clusters_for_delete.snapshot();
								match snapshot.status {
									QueryStatus::Success => {
										let items = snapshot.data.unwrap_or_default();
										let selection_changed = props.delete_selection_changed;
										let notice = self::query_refetch_notice(
											snapshot.is_fetching,
											snapshot.refetch_error,
											"clusters",
										);
										let selector = self::entity_select(
											"Cluster",
											"Select cluster",
											self::cluster_select_options(&items),
											props.delete_cluster_id,
											move |value| selection_changed.call(value),
										);
										page!({
											{ notice }
											{ selector }
										})
									}
									QueryStatus::Idle => page!({
										p {
											class: "mb-3 text-xs text-cloud-500",
											"Clusters are not available during server rendering."
										}
									}),
									QueryStatus::Pending => page!({
										p {
											class: "mb-3 text-xs text-ink-600",
											"Loading clusters..."
										}
									}),
									QueryStatus::Error => {
										let message = self::query_error_message(
											snapshot.error,
											"Clusters are temporarily unavailable.",
										);
										page!({
											p {
												class: "mb-3 text-xs font-medium text-red-700",
												{ message }
											}
										})
									}
								}
							} {
								props.delete_view.clone()
							}
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
	use std::cell::Cell;
	#[cfg(native)]
	use std::rc::Rc;

	use reinhardt::pages::reactive::ReactiveScope;
	use rstest::rstest;

	use super::*;

	#[cfg(native)]
	#[rstest]
	fn server_rendering_actions_share_the_operation_specific_error() {
		// Arrange
		let operation = "token rotation";

		// Act
		let error = unavailable_during_server_rendering::<()>(operation)
			.expect_err("server rendering must reject cluster mutations");

		// Assert
		assert_eq!(
			error.message(),
			"Cluster token rotation is unavailable during server rendering"
		);
	}

	#[rstest]
	fn test_cluster_create_error_routing_preserves_structured_fields() {
		ReactiveScope::run(|| {
			// Arrange
			let form = form! {
				name: CreateClusterStructuredErrorForm,
				model: ClusterCreateForm,
				policy: ClusterCreateFields,
				fields: [name, api_url],
				server_fn: create_cluster_for_current_org,
			};
			let runtime = use_form(&form).build();
			let error = ServerFnError::validation_with_message(
				"Please correct the cluster form",
				[
					("name", "A cluster with this name already exists"),
					("api_url", "Enter a valid Kubernetes API URL"),
					("organization_id", "This field is managed by the server"),
				],
			);

			// Act
			runtime.apply_server_error(&error);

			// Assert
			assert_eq!(
				runtime
					.get_field_state(form.name_field())
					.error
					.as_ref()
					.map(FieldError::message),
				Some("A cluster with this name already exists")
			);
			assert_eq!(
				runtime
					.get_field_state(form.api_url_field())
					.error
					.as_ref()
					.map(FieldError::message),
				Some("Enter a valid Kubernetes API URL")
			);
			assert_eq!(
				runtime.form_state().form_error.get(),
				Some(
					"Please correct the cluster form\norganization_id: This field is managed by the server"
						.to_string()
				)
			);
		});
	}

	#[rstest]
	fn test_cluster_update_client_form_routes_structured_server_errors() {
		ReactiveScope::run(|| {
			// Arrange
			let form =
				UpdateClusterFormRequestClientForm::new().with_defaults(UpdateClusterFormRequest {
					cluster_id: "41".to_owned(),
					name: "production".to_owned(),
					api_url: "https://kubernetes.example.com:6443".to_owned(),
					is_active: true,
				});
			let runtime = use_form(&form).build();
			let error = ServerFnError::validation_with_message(
				"Please correct the cluster form",
				[
					("name", "A cluster with this name already exists"),
					("organization_id", "This field is managed by the server"),
				],
			);

			// Act
			runtime.apply_server_error(&error);

			// Assert
			assert_eq!(
				runtime
					.get_field_state(UpdateClusterFormRequestClientFormField::Name)
					.error
					.as_ref()
					.map(FieldError::message),
				Some("A cluster with this name already exists")
			);
			assert_eq!(
				runtime.form_state().form_error.get(),
				Some(
					"Please correct the cluster form\norganization_id: This field is managed by the server"
						.to_owned()
				)
			);
		});
	}

	#[cfg(native)]
	#[rstest]
	#[tokio::test]
	async fn test_cluster_update_client_form_blocks_invalid_submit_before_server_dispatch() {
		// Arrange
		let scope = ReactiveScope::new();
		let runtime = scope.enter(|| {
			let form = UpdateClusterFormRequestClientForm::new();
			use_form(&form).build()
		});
		let submit_calls = Rc::new(Cell::new(0));
		let submit_calls_for_server_fn = Rc::clone(&submit_calls);

		// Act
		let outcome = runtime
			.submit_server_fn(move || {
				submit_calls_for_server_fn.set(submit_calls_for_server_fn.get() + 1);
				async {
					Ok::<_, ServerFnError>(ClusterInfo {
						id: 1,
						name: "unused".to_owned(),
						api_url: "https://unused.example.com".to_owned(),
						is_active: true,
						token_last_rotated_at: None,
					})
				}
			})
			.await
			.expect("validation rejection is a submit outcome");

		// Assert
		assert_eq!(outcome, UseFormAsyncSubmitOutcome::ValidationFailed);
		assert_eq!(submit_calls.get(), 0);
	}
}
