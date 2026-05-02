//! Type-safe URL accessors for the `dashboard` SPA routes.

const HOME_PATH: &str = "/";
const CLUSTERS_PATH: &str = "/clusters";
const DEPLOYMENTS_PATH: &str = "/deployments";

/// Return the URL path for the `dashboard:home` SPA route.
pub fn home() -> &'static str {
	HOME_PATH
}

/// Return the URL path for the `dashboard:clusters` SPA route.
pub fn clusters() -> &'static str {
	CLUSTERS_PATH
}

/// Return the URL path for the `dashboard:deployments` SPA route.
pub fn deployments() -> &'static str {
	DEPLOYMENTS_PATH
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	#[case::home(home(), "/")]
	#[case::clusters(clusters(), "/clusters")]
	#[case::deployments(deployments(), "/deployments")]
	fn dashboard_routes_return_expected_paths(
		#[case] actual: &'static str,
		#[case] expected: &'static str,
	) {
		// Arrange — covered by #[case]
		// Act — covered by #[case]
		// Assert
		assert_eq!(actual, expected);
	}
}
