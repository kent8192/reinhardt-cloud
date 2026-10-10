//! Client-side routing for the accounts SPA.
//!
//! Route names are defined by this app's route-backed client components and
//! registered only in client builds.

use reinhardt::ClientRouter;

use crate::apps::accounts::client::components;

pub fn client_url_patterns() -> ClientRouter {
	ClientRouter::new()
		.component(components::sign_in::sign_in)
		.component(components::home::home)
}

pub fn reverse(name: &str, params: &[(&str, &str)]) -> String {
	client_url_patterns()
		.reverse(name, params)
		.unwrap_or_else(|error| panic!("failed to reverse accounts client route `{name}`: {error}"))
}
