//! Client-side routing for the accounts SPA.
//!
//! Route names are defined by this app's route-backed client components and
//! registered only in client builds.

use reinhardt::ClientRouter;

pub fn client_url_patterns() -> ClientRouter {
	ClientRouter::new()
}

pub fn reverse(name: &str, params: &[(&str, &str)]) -> String {
	client_url_patterns()
		.reverse(name, params)
		.unwrap_or_else(|error| panic!("failed to reverse accounts client route `{name}`: {error}"))
}
