//! Client route reverse helpers shared across dashboard pages.

pub(crate) fn route_href(route_name: &'static str, fallback: &'static str) -> String {
	#[cfg(wasm)]
	{
		crate::client::router::init_router()
			.reverse(route_name, &[])
			.unwrap_or_else(|_| fallback.to_string())
	}
	#[cfg(not(wasm))]
	{
		let _ = route_name;
		fallback.to_string()
	}
}

#[cfg(test)]
mod tests {
	use rstest::rstest;

	#[rstest]
	fn route_href_uses_fallback_on_native() {
		// Arrange
		let route_name = "auth:login_page";
		let fallback = "/login";

		// Act
		let href = super::route_href(route_name, fallback);

		// Assert
		assert_eq!(href, "/login");
	}
}
