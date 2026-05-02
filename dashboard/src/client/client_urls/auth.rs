//! Type-safe URL accessors for the `auth` SPA routes.

const LOGIN_PAGE_PATH: &str = "/login";
const REGISTER_PAGE_PATH: &str = "/register";

/// Return the URL path for the `auth:login_page` SPA route.
pub fn login_page() -> &'static str {
	LOGIN_PAGE_PATH
}

/// Return the URL path for the `auth:register_page` SPA route.
pub fn register_page() -> &'static str {
	REGISTER_PAGE_PATH
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	#[case::login(login_page(), "/login")]
	#[case::register(register_page(), "/register")]
	fn auth_routes_return_expected_paths(
		#[case] actual: &'static str,
		#[case] expected: &'static str,
	) {
		// Arrange — covered by #[case]
		// Act — covered by #[case]
		// Assert
		assert_eq!(actual, expected);
	}
}
