//! Dashboard shell layout with header, sidebar, and main content area.

use reinhardt::pages::component::Page;
use reinhardt::pages::page;

use crate::client::client_urls;

/// Render the main dashboard shell with navigation sidebar and overview cards.
pub fn dashboard_shell() -> Page {
	page!(|| {
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
						href: client_urls::auth::login_page(),
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
								href: client_urls::dashboard::home(),
								class: "block px-3 py-2 text-sm rounded-md bg-blue-50 text-blue-700 font-medium",
								"Overview"
							}
						}
						li {
							a {
								href: client_urls::dashboard::clusters(),
								class: "block px-3 py-2 text-sm rounded-md text-gray-700 hover:bg-gray-100",
								"Clusters"
							}
						}
						li {
							a {
								href: client_urls::dashboard::deployments(),
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
	})()
}
