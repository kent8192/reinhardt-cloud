//! Shared server-side authentication for typed Pages functions.
#[cfg(server)]
use super::persistence::Actor;
#[cfg(server)]
use crate::apps::organization::persistence::OrganizationError;
#[cfg(server)]
use reinhardt::pages::server_fn::{ServerFnError, ServerFnRequest};
#[cfg(server)]
use uuid::Uuid;
#[cfg(server)]
pub fn actor(request: &ServerFnRequest) -> Result<Uuid, ServerFnError> {
	request
		.inner()
		.extensions
		.get::<Actor>()
		.map(|a| a.id)
		.ok_or_else(|| ServerFnError::auth(401, "Sign in to continue"))
}
#[cfg(server)]
pub fn read_error(error: OrganizationError) -> ServerFnError {
	match error {
		OrganizationError::Forbidden => ServerFnError::auth(403, "Access denied"),
		_ => ServerFnError::server(503, "The service is unavailable. Try again later."),
	}
}
