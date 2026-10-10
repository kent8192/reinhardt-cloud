#![cfg(not(target_arch = "wasm32"))]

#[path = "e2e/auth_dashboard_clusters_deployments_github/browser_session.rs"]
mod browser_session;

#[path = "e2e/auth_dashboard_clusters_deployments_github/test_dashboard_pages.rs"]
mod test_dashboard_pages;
