//! Organization selection from authenticated membership projections.
use crate::apps::organization::functions::load_organizations;
use crate::apps::organization::services::OrganizationSummary;
use crate::client::components::shell::{STYLES, shell};
use crate::client::navigation::{projects_path, route_link};
use crate::client::screens::{context, csrf_token, loading, query_error, workspace};
use reinhardt::pages::i18n::I18nContext;
use reinhardt::pages::{Page, QueryOptions, component, page, use_query};
#[component("/organizations/", name = "organization-chooser")]
pub fn organizations() -> Page {
	let context = context();
	let query = use_query(load_organizations::query(), QueryOptions::default());
	let content = page!({
		if let Some(items) = query.data() {
			{ chooser(items.clone(), context.clone()) }
		} else if let Some(error) = query.error() {
			{ query_error(error.clone(), context.clone()) }
		} else {
			{ loading(context.clone()) }
		}
	});
	shell(
		workspace(None, content, true, context.clone(), csrf_token()),
		context,
	)
}
pub fn chooser(organizations: Vec<OrganizationSummary>, context: I18nContext) -> Page {
	page!({
		h2 {
			class: STYLES.panel_title(),
			{ context.translate("Choose an organization") }
		}
		ul {
			for organization in organizations @key(organization.id) {
				li { { route_link(projects_path(organization.id), organization.name.clone()) } }
			}
		}
	})
}
