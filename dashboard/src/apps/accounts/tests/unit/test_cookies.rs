//! Cookie attributes and parsing (SR-04, SR-08).

use std::time::Duration;

use rstest::rstest;

use crate::apps::accounts::server::cookies::{CookieSecurity, cookie_value};

#[rstest]
#[case::plain(
	false,
	"cloud_session=abc; HttpOnly; SameSite=Lax; Path=/; Max-Age=86400"
)]
#[case::secure(
	true,
	"cloud_session=abc; HttpOnly; SameSite=Lax; Path=/; Max-Age=86400; Secure"
)]
fn sr_08_a_cookie_is_script_inaccessible_lax_and_secure_where_configured(
	#[case] secure: bool,
	#[case] expected: &str,
) {
	// Arrange
	let security = CookieSecurity { secure };

	// Act
	let cookie = security.set("cloud_session", "abc", "/", Duration::from_secs(86_400));

	// Assert
	assert_eq!(cookie, expected);
}

#[rstest]
fn sr_08_clearing_a_cookie_expires_it_on_the_same_path() {
	// Arrange
	let security = CookieSecurity { secure: true };

	// Act
	let cookie = security.clear("cloud_signin_binding", "/api/auth/github/callback/");

	// Assert
	assert_eq!(
		cookie,
		"cloud_signin_binding=; HttpOnly; SameSite=Lax; Path=/api/auth/github/callback/; Max-Age=0; Secure"
	);
}

#[rstest]
#[case("a=1; cloud_session=tok; b=2", "cloud_session", Some("tok"))]
#[case("cloud_session=tok", "cloud_session", Some("tok"))]
#[case("xcloud_session=evil; cloud_session=tok", "cloud_session", Some("tok"))]
#[case("cloud_session=", "cloud_session", None)]
#[case("other=1", "cloud_session", None)]
#[case("", "cloud_session", None)]
#[case("cloud_session", "cloud_session", None)]
fn a_cookie_is_found_by_exact_name(
	#[case] header: &str,
	#[case] name: &str,
	#[case] expected: Option<&str>,
) {
	// Arrange / Act
	let found = cookie_value(header, name);

	// Assert
	assert_eq!(found, expected);
}
