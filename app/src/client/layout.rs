//! Dashboard shell layout with header, sidebar, and main content area.

use reinhardt::pages::component::Page;
use reinhardt::pages::page;

/// Render the main dashboard shell with navigation sidebar and overview cards.
pub fn dashboard_shell() -> Page {
	page!(|| {
		div {
			class: "min-h-screen flex flex-col bg-gray-50",
			// Header
			header {
				class: "h-14 bg-white border-b border-gray-200 flex items-center justify-between px-6 shrink-0",
				div {
					class: "flex items-center gap-2",
					span {
						class: "text-lg font-bold text-blue-600",
						"Nuages"
					}
				}
				div {
					class: "flex items-center gap-4",
					span {
						class: "text-sm text-gray-600",
						"Dashboard"
					}
					a {
						href: "/login",
						class: "text-sm text-blue-600 hover:underline",
						"Login"
					}
				}
			}
			// Body: sidebar + main
			div {
				class: "flex flex-1 overflow-hidden",
				// Sidebar
				nav {
					class: "w-56 bg-white border-r border-gray-200 py-4 shrink-0",
					div {
						class: "px-4 mb-2",
						span {
							class: "text-xs font-semibold text-gray-400 uppercase tracking-wider",
							"Navigation"
						}
					}
					a {
						href: "/",
						class: "block px-4 py-2 text-sm font-medium text-blue-600 bg-blue-50 border-r-2 border-blue-600",
						"Dashboard"
					}
					div {
						class: "block px-4 py-2 text-sm text-gray-400 cursor-not-allowed",
						"Clusters (coming soon)"
					}
					div {
						class: "block px-4 py-2 text-sm text-gray-400 cursor-not-allowed",
						"Deployments (coming soon)"
					}
				}
				// Main content
				main {
					class: "flex-1 p-8 overflow-y-auto",
					h1 {
						class: "text-2xl font-bold text-gray-800 mb-6",
						"Overview"
					}
					div {
						class: "grid grid-cols-1 md:grid-cols-3 gap-6",
						// Clusters card
						div {
							class: "bg-white rounded-lg border border-gray-200 p-6",
							p {
								class: "text-sm font-medium text-gray-500 mb-1",
								"Clusters"
							}
							p {
								class: "text-3xl font-bold text-gray-800",
								"\u{2014}"
							}
						}
						// Deployments card
						div {
							class: "bg-white rounded-lg border border-gray-200 p-6",
							p {
								class: "text-sm font-medium text-gray-500 mb-1",
								"Deployments"
							}
							p {
								class: "text-3xl font-bold text-gray-800",
								"\u{2014}"
							}
						}
						// Users card
						div {
							class: "bg-white rounded-lg border border-gray-200 p-6",
							p {
								class: "text-sm font-medium text-gray-500 mb-1",
								"Users"
							}
							p {
								class: "text-3xl font-bold text-gray-800",
								"\u{2014}"
							}
						}
					}
				}
			}
		}
	})()
}
