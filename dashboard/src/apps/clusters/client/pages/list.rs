//! Clusters list page.
//!
//! The dedicated list view has not yet been implemented; the handler
//! delegates to the shared `not_found_page` so the typed `clusters:list`
//! reverse URL accessor and the SPA navigation chain stay wired up.

use reinhardt::pages::component::Page;

use crate::client::pages::not_found_page;

/// Render the clusters list page placeholder.
pub fn clusters_list_page() -> Page {
	not_found_page()
}
