//! URL configuration for the auth app.
//!
//! Server endpoints and client SPA routes are merged into the single
//! `url_patterns()` function below as a `UnifiedRouter`. WebSocket
//! patterns live in `ws_urls.rs` because the `#[routes]` macro
//! discovers them at the fixed path
//! `crate::apps::<app>::urls::ws_urls::ws_url_resolvers`.

pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

use crate::apps::auth::client::pages::{login_page, register_page};
#[cfg(native)]
use crate::apps::auth::views;

/// Returns the unified URL patterns for the auth app.
///
/// Combines server endpoints (REST API + server functions) and the SPA
/// client route table in a single `UnifiedRouter`. Named routes
/// declared inside `.client(|c| c.route("login_page", ...))` are
/// globally reversible as `auth:login_page` after `mount_unified`
/// merges them into the project router (kent8192/reinhardt-web#4077).
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
		.server(|s| {
			#[cfg(native)]
			let s = s.endpoint(views::login)
				.endpoint(views::register)
				.endpoint(views::verify_email)
				.endpoint(views::forgot_password)
				.endpoint(views::reset_password)
				.endpoint(views::profile)
				.endpoint(views::profile_update)
				.endpoint(views::change_password)
				.endpoint(views::verify_email_change)
				.endpoint(views::oauth_start)
				.endpoint(views::oauth_callback)
				.endpoint(views::oauth_unlink)
				.endpoint(views::oauth_providers);
			s
		})
		.client(|c| {
			c.route("login_page", "/login", login_page).route(
				"register_page",
				"/register",
				register_page,
			)
		})
}
