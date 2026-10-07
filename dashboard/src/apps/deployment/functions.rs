//! Runtime operation acceptance through a typed, session-authorized boundary.
use super::services::{OperationProgress, OperationRequest};
#[cfg(server)]
use reinhardt::pages::server_fn::ServerFnRequest;
use reinhardt::pages::server_fn::{ServerFnError, server_fn};
#[server_fn(endpoint = "/api/server_fn/runtime/")]
pub async fn change_runtime(
	input: OperationRequest,
	#[inject] request: ServerFnRequest,
) -> Result<OperationProgress, ServerFnError> {
	let actor = crate::apps::identity::functions::actor(&request)?;
	let connection = reinhardt::db::orm::manager::get_connection()
		.await
		.map_err(|_| {
			ServerFnError::server(503, "Operation storage is unavailable. Try again later.")
		})?;
	super::persistence::accept_operation(&connection, actor, &input)
		.await
		.map_err(|error| {
			use super::persistence::AcceptanceError;
			match error {
				AcceptanceError::Forbidden => ServerFnError::auth(403, "Access denied"),
				AcceptanceError::InvalidRequest => ServerFnError::application_with_status(
					422,
					"Check the runtime settings and try again.",
				),
				AcceptanceError::VersionConflict
				| AcceptanceError::IdempotencyConflict
				| AcceptanceError::EnvironmentBusy => ServerFnError::application_with_status(
					409,
					"The Environment changed or has an unfinished operation. Refresh before trying again.",
				),
				AcceptanceError::NotReady => ServerFnError::application_with_status(
					409,
					"The cluster cannot accept this runtime change.",
				),
				_ => {
					ServerFnError::server(503, "Operation storage is unavailable. Try again later.")
				}
			}
		})
}
