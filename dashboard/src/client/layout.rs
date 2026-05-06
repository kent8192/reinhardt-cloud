//! Dashboard shell layout with header, sidebar, and main content area.

use reinhardt::pages::component::Page;
use reinhardt::pages::page;

use crate::config::urls::ResolvedUrls;

/// Render the main dashboard shell with navigation sidebar and overview cards.
pub fn dashboard_shell() -> Page {
	// Resolve all SPA URLs once per render. `ResolvedUrls::from_global()`
	// clones two `Arc`s on each call, so we hoist it out of the rsx tree
	// to avoid repeating that work for every `href`.
	let urls = ResolvedUrls::from_global();
	let login_href = urls.client().auth().login_page();
	let home_href = urls.client().dashboard().home();
	let clusters_href = urls.client().dashboard().clusters();
	let deployments_href = urls.client().dashboard().deployments();
	page!(
		|login_href: String,
		 home_href: String,
		 clusters_href: String,
		 deployments_href: String| {
			div {
				class: "min-h-screen flex flex-col bg-gray-50",
				header {
					class: "h-14 bg-white border-b border-gray-200 flex items-center justify-between px-6 shrink-0",
					div {
						class: "flex items-center gap-2",
						span {
							class: "text-lg font-bold text-blue-600",
							"Reinhardt Cloud"
						}
					}
					div {
						class: "flex items-center gap-4",
						span {
							class: "text-sm text-gray-600",
							"Dashboard"
						}
						a {
							href: login_href,
							class: "text-sm text-blue-600 hover:underline",
							"Login"
						}
					}
				}
				div {
					class: "flex flex-1",
					nav {
						class: "w-56 bg-white border-r border-gray-200 p-4 shrink-0",
						ul {
							class: "space-y-1",
							li {
								a {
									href: home_href,
									class: "block px-3 py-2 text-sm rounded-md bg-blue-50 text-blue-700 font-medium",
									"Overview"
								}
							}
							li {
								a {
									href: clusters_href,
									class: "block px-3 py-2 text-sm rounded-md text-gray-700 hover:bg-gray-100",
									"Clusters"
								}
							}
							li {
								a {
									href: deployments_href,
									class: "block px-3 py-2 text-sm rounded-md text-gray-700 hover:bg-gray-100",
									"Deployments"
								}
							}
						}
					}
				main {
					class: "flex-1 p-6",
					h1 {
						class: "text-2xl font-bold text-gray-900 mb-6",
						"Dashboard"
					}
					div {
						class: "grid grid-cols-1 md:grid-cols-3 gap-6",
						div {
							class: "bg-white rounded-lg border border-gray-200 p-6",
							h3 {
								class: "text-sm font-medium text-gray-500 mb-1",
								"Clusters"
							}
							p {
								class: "text-3xl font-bold text-gray-900",
								"0"
							}
						}
						div {
							class: "bg-white rounded-lg border border-gray-200 p-6",
							h3 {
								class: "text-sm font-medium text-gray-500 mb-1",
								"Deployments"
							}
							p {
								class: "text-3xl font-bold text-gray-900",
								"0"
							}
						}
						div {
							class: "bg-white rounded-lg border border-gray-200 p-6",
							h3 {
								class: "text-sm font-medium text-gray-500 mb-1",
								"System Status"
							}
							p {
								class: "text-lg font-semibold text-green-600",
								"Healthy"
							}
						}
					}
				}
			}
		}
	}
	)(login_href, home_href, clusters_href, deployments_href)
}
