//! Typed organization queries authenticated by the live browser session.
use super::services::OrganizationSummary;
#[cfg(server)]
use reinhardt::pages::server_fn::ServerFnRequest;
use reinhardt::pages::server_fn::{ServerFnError, server_fn};
#[server_fn(endpoint = "/api/server_fn/organizations/")]
pub async fn load_organizations(
	#[inject] request: ServerFnRequest,
) -> Result<Vec<OrganizationSummary>, ServerFnError> {
	let actor = crate::apps::identity::functions::actor(&request)?;
	super::persistence::organizations_for(actor)
		.await
		.map_err(crate::apps::identity::functions::read_error)
}
