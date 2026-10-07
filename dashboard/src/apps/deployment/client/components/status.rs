//! Operation progress is distinct from observed Environment readiness.
use crate::apps::deployment::services::{OperationSnapshot, OperationState};
use crate::client::components::shell::STYLES;
use reinhardt::pages::i18n::I18nContext;
use reinhardt::pages::{Page, page};
pub fn operation_status(latest_operation: Option<OperationSnapshot>, context: I18nContext) -> Page {
	match latest_operation {
		None => page!({
			p {
				class: STYLES.copy(),
				{ context.translate("No operations yet") }
			}
		}),
		Some(snapshot) => {
			let state = match snapshot.progress.state {
				OperationState::Queued => "Queued",
				OperationState::Building => "Building",
				OperationState::Migrating => "Migrating",
				OperationState::Applying => "Applying",
				OperationState::Verifying => "Verifying",
				OperationState::Uncertain => "Outcome uncertain",
				OperationState::Succeeded => "Succeeded",
				OperationState::Failed => "Failed",
				OperationState::Cancelled => "Cancelled",
			};
			let kind = match snapshot.kind.as_str() {
				"restart" => "Restart",
				"scale" => "Scale",
				"autoscale" => "CPU autoscaling",
				_ => "Operation",
			};
			page!({
				h4 {
					class: STYLES.operation_title(),
					{ context.translate("Latest operation") }
				}
				p {
					class: STYLES.status(),
					{ context.translate(kind) }": " { context.translate(state) }
				}
				p {
					class: STYLES.identity(),
					{ format!("{}", snapshot.progress.id) }
				}
				time {
					datetime: snapshot.created_at.to_rfc3339(),
					{ snapshot.created_at.format("%Y-%m-%d %H:%M UTC").to_string() }
				}
				if snapshot.progress.state == OperationState::Uncertain {
					p {
						role: "status",
						class: STYLES.copy(),
						{ context.translate("Further changes wait for cluster reconciliation.") }
					}
				}
			})
		}
	}
}
