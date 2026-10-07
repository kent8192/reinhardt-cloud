//! Duplicate-safe runtime mutation with version fencing and query invalidation.
use crate::apps::deployment::{
	functions::change_runtime,
	services::{OperationRequest, RuntimeChange},
};
use crate::client::components::shell::STYLES;
use reinhardt::pages::component::{IntoPage, PageElement};
use reinhardt::pages::i18n::I18nContext;
use reinhardt::pages::{NumberParseError, Page, Signal, page, style_def, use_server_mutation};
use uuid::Uuid;

#[style_def]
static CONTROL_STYLES: RuntimeControlStyles = style! {
	.group {
		display: grid;
		gap: 12px;
		min-width: 0;
		margin: (24px, 0, 0);
		padding: 0;
		border: 0;
		legend {
			margin-bottom: 12px;
			padding: 0;
			font-size: 13px;
			font-weight: 600;
		}
		&:disabled {
			opacity: 0.6;
			button {
				cursor: not-allowed;
			}
			input {
				cursor: not-allowed;
			}
		}
	}
};

pub fn runtime_controls(
	organization_id: Uuid,
	project_id: Uuid,
	environment_id: Uuid,
	expected_version: i64,
	desired_replicas: u32,
	blocked: bool,
	context: I18nContext,
) -> Page {
	let builder = use_server_mutation(change_runtime::mutation());
	#[cfg(wasm)]
	let builder = if reinhardt::pages::app::try_with_spa_router(|_| ()).is_some() {
		builder.invalidate(
			reinhardt::pages::queries(),
			crate::apps::project::functions::load_project::key(organization_id, project_id),
		)
	} else {
		builder.on_success(|_| {
			if let Some(window) = web_sys::window() {
				let _ = window.location().reload();
			}
		})
	};
	#[cfg(server)]
	let _ = project_id;
	let mutation = builder.build();
	let replicas = Signal::new(desired_replicas);
	let replicas_error = Signal::new(None::<NumberParseError>);
	let last_request = Signal::new(None::<OperationRequest>);
	let validation = Signal::new(false);
	// Attribute effects observe mutation state without subscribing the parent route render.
	let controls = PageElement::new("fieldset")
		.attr("class", CONTROL_STYLES.group())
		.reactive_attr("disabled", move || {
			(blocked || mutation.is_pending() || mutation.is_success()).then(|| "disabled".into())
		})
		.child(page!({
			legend { { context.translate("Runtime controls") } }
			label {
				for: format!("replicas-{environment_id}"),
				{ context.translate("Desired replicas") }
			}
			input {
				id: format!("replicas-{environment_id}"),
				type: "number",
				class: STYLES.input(),
				min: 1,
				max: 1000,
				bind: number(replicas, replicas_error),
			}
			button {
				type: "button",
				class: STYLES.primary(),
				@click: move |_event: reinhardt::pages::event::ClickEvent| {
					let replicas = replicas.get_untracked();
					if replicas_error.get_untracked().is_none() && (1..=1000).contains(&replicas) {
						validation.set(false);
						mutation.dispatch(operation_request(
							last_request,
							OperationRequest {
								organization_id,
								environment_id,
								expected_version,
								idempotency_key: String::new(),
								change: RuntimeChange::Scale { replicas },
							},
						));
					} else {
						validation.set(true);
					}
				},
				{ context.translate("Scale") }
			}
			button {
				type: "button",
				class: STYLES.language(),
				@click: move |_event: reinhardt::pages::event::ClickEvent| {
					mutation.dispatch(operation_request(
						last_request,
						OperationRequest {
							organization_id,
							environment_id,
							expected_version,
							idempotency_key: String::new(),
							change: RuntimeChange::Restart,
						},
					));
				},
				{ context.translate("Restart") }
			}
		}))
		.into_page();
	page!({
		{ controls }
		if validation.get() {
			p {
				role: "alert",
				{ context.translate("Enter a replica count between 1 and 1000.") }
			}
		}
		if mutation.is_pending() {
			p {
				role: "status",
				{ context.translate("Submitting…") }
			}
		}
		if mutation.is_success() {
			p {
				role: "status",
				{ context.translate("Operation queued. Readiness is reported separately.") }
			}
		}
		if let Some(error) = mutation.error() {
			p {
				role: "alert",
				{ error.to_string() }
			}
		}
	})
}
/// Keep the original key for retries; changed inputs receive a fresh identity.
fn operation_request(
	last: Signal<Option<OperationRequest>>,
	mut input: OperationRequest,
) -> OperationRequest {
	if let Some(previous) = last.get()
		&& previous.change == input.change
		&& previous.expected_version == input.expected_version
	{
		return previous;
	}
	input.idempotency_key = Uuid::new_v4().to_string();
	last.set(Some(input.clone()));
	input
}
