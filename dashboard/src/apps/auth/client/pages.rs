//! Auth page components for the WASM client.

pub mod login;
pub mod register;

pub use login::login_page;
pub use register::register_page;

pub(crate) fn auth_href(route_name: &str, fallback: &str) -> String {
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
	use super::*;

	#[rstest::rstest]
	fn auth_href_uses_fallback_on_native() {
		// Arrange
		let route_name = "auth:login_page";
		let fallback = "/login";

		// Act
		let href = auth_href(route_name, fallback);

		// Assert
		assert_eq!(href, "/login");
	}
}
