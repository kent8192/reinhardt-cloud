//! Persistent organization shell for nested Project routes.
use crate::client::components::shell::shell;
use crate::client::screens::context;
use reinhardt::pages::{Outlet, Page, layout, page};
#[layout("/organizations/{organization_id}/", name = "organization-shell")]
pub fn organization_shell(outlet: Outlet) -> Page {
	shell(page!({ { outlet } }), context())
}
