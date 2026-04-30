//! Cross-target SPA route table — the single source of truth for SPA
//! URL patterns shared by the WASM `Router` and the native
//! `DashboardUrlResolver`.
//!
//! # Workaround for kent8192/reinhardt-web#4067 (tracked in #501)
//!
//! Upstream's `UnifiedRouter::client(...)` would let the server-side
//! build call `ClientRouter::reverse(name, params)` directly. That is
//! currently incompatible with our DI router pattern: enabling the
//! `client-router` feature pulls a non-`Sync` `ClientRouter` into
//! `UnifiedRouter`, breaking `Depends<DashboardRouter>` resolution
//! through `InjectionContext::resolve` (which requires `Send + Sync`).
//!
//! Until upstream resolves this, the same `(name, pattern)` pairs are
//! consumed by both:
//! - `super::router::init_router` (WASM-only — binds names to page
//!   components for the SPA).
//! - `super::url::DashboardUrlResolver::resolve_client_url` on native —
//!   reverse URL lookup for SSR `href` generation.
//!
//! Ideal implementation (without workaround):
//!
//! ```rust,ignore
//! // In dashboard/src/config/urls.rs::make_router (once #4067 lands):
//! .client(|c: ClientRouter| {
//!     c.named_route("dashboard:home", "/", dashboard_shell)
//!         .named_route("auth:login_page", "/login", login_page)
//!         .not_found(not_found_page)
//! })
//!
//! // The native branch of DashboardUrlResolver becomes:
//! reinhardt::get_client_reverser()
//!     .and_then(|r| r.reverse(name, params).ok())
//!     .unwrap_or_else(|| panic!("SPA route '{name}' not registered"))
//! ```
//!
//! # Adding a route
//!
//! Append to [`SPA_ROUTES`] **and** add the matching `named_route(...)`
//! call in [`super::router::init_router`]. The
//! `route_table_matches_init_router` test verifies the two stay in sync
//! on WASM builds.

/// Named SPA routes as `(route_name, path_pattern)` pairs.
///
/// Currently no routes use path parameters; if that changes, extend
/// [`super::url::DashboardUrlResolver`] to substitute `{param}`
/// placeholders before returning the resolved path.
pub const SPA_ROUTES: &[(&str, &str)] = &[
	("dashboard:home", "/"),
	("auth:login_page", "/login"),
	("auth:register_page", "/register"),
	// Placeholder names so navigation hrefs resolve via
	// `ClientUrlResolver` even before these pages are implemented.
	("dashboard:clusters", "/clusters"),
	("dashboard:deployments", "/deployments"),
];

/// Look up a route pattern by name. Returns `None` if the name is not
/// registered.
pub fn lookup(name: &str) -> Option<&'static str> {
	SPA_ROUTES
		.iter()
		.find_map(|&(n, p)| (n == name).then_some(p))
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	#[case::home("dashboard:home", "/")]
	#[case::login("auth:login_page", "/login")]
	#[case::register("auth:register_page", "/register")]
	#[case::clusters("dashboard:clusters", "/clusters")]
	#[case::deployments("dashboard:deployments", "/deployments")]
	fn lookup_returns_pattern_for_registered_name(
		#[case] name: &str,
		#[case] expected_pattern: &str,
	) {
		// Arrange & Act
		let resolved = lookup(name);

		// Assert
		assert_eq!(
			resolved,
			Some(expected_pattern),
			"SPA_ROUTES must contain '{name}' → '{expected_pattern}' so server-side reverse URL resolution works"
		);
	}

	#[rstest]
	fn lookup_returns_none_for_unregistered_name() {
		// Arrange & Act
		let resolved = lookup("nonexistent:route");

		// Assert
		assert!(
			resolved.is_none(),
			"lookup must return None for unregistered names so callers can fail fast"
		);
	}

	#[rstest]
	fn spa_routes_have_unique_names() {
		// Arrange & Act
		let mut names: Vec<&str> = SPA_ROUTES.iter().map(|&(n, _)| n).collect();
		names.sort_unstable();
		let total = names.len();
		names.dedup();

		// Assert
		assert_eq!(
			names.len(),
			total,
			"SPA_ROUTES must not contain duplicate route names; lookup() returns the first match and would silently shadow later entries"
		);
	}
}
