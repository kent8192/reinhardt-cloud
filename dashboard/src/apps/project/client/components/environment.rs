//! Reusable Environment summary composed with deployment-owned controls.
use crate::apps::deployment::client::components::{
	controls::runtime_controls, status::operation_status,
};
use crate::apps::deployment::services::OperationState;
use crate::apps::project::services::{EnvironmentKind, EnvironmentSummary};
use crate::client::components::shell::STYLES;
use reinhardt::pages::i18n::I18nContext;
use reinhardt::pages::{Page, page};
pub fn environment_card(
	organization_id: uuid::Uuid,
	project_id: uuid::Uuid,
	environment: EnvironmentSummary,
	context: I18nContext,
) -> Page {
	let EnvironmentSummary {
		id,
		kind: environment_kind,
		version,
		desired_runtime,
		latest_operation,
	} = environment;
	let kind = match environment_kind {
		EnvironmentKind::Production => "Production",
		EnvironmentKind::Staging => "Staging",
		EnvironmentKind::Preview => "Preview",
	};
	let blocked = latest_operation.as_ref().is_some_and(|operation| {
		!matches!(
			operation.progress.state,
			OperationState::Succeeded | OperationState::Failed | OperationState::Cancelled
		)
	});
	let operation = operation_status(latest_operation, context.clone());
	let controls = runtime_controls(
		organization_id,
		project_id,
		id,
		version,
		desired_runtime.replicas,
		blocked,
		context.clone(),
	);
	let replicas = desired_runtime.replicas;
	let autoscaling = desired_runtime.autoscaling;
	let autoscaling_enabled = autoscaling.is_some();
	let range = autoscaling
		.as_ref()
		.map(|configuration| {
			format!(
				"{}–{} · {}% CPU",
				configuration.min_replicas,
				configuration.max_replicas,
				configuration.target_cpu_percent
			)
		})
		.unwrap_or_default();
	page!({
		article {
			class: STYLES.environment_card(),
			h3 {
				class: STYLES.panel_title(),
				{ context.translate(kind) }
			}
			p {
				class: STYLES.identity(),
				{ format!("{}", id) }
			}
			dl {
				class: STYLES.facts(),
				dt { { context.translate("Configuration version") } }
				dd { { format!("{}", version) } }
				dt { { context.translate("Desired replicas") } }
				dd { { format!("{}", replicas) } }
				if autoscaling_enabled {
					dt { { context.translate("CPU autoscaling") } }
					dd { { range } }
				}
			}
			{ operation }
			{ controls }
		}
	})
}
