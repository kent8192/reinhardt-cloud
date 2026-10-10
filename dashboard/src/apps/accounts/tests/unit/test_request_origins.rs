//! The allow-list of cross-site request origins (SR-12) and the GitHub App
//! configuration the same settings describe.

use reinhardt::conf::settings::fragment::SettingsValidation;
use reinhardt::conf::settings::profile::Profile;
use reinhardt::conf::settings::secret_types::SecretString;
use rstest::rstest;

use crate::apps::accounts::server::settings::AccountsSettings;

fn settings(public_url: &str, allowed_origins: &str) -> AccountsSettings {
	AccountsSettings {
		public_url: public_url.to_owned(),
		allowed_origins: allowed_origins.to_owned(),
		..AccountsSettings::default()
	}
}

#[rstest]
fn sr_12_the_public_url_and_the_configured_origins_are_allowed() {
	// Arrange
	let settings = settings(
		"https://reinhardt-cloud.dev/",
		"https://www.reinhardt-cloud.dev, https://reinhardt-cloud.dev",
	);

	// Act
	let origins = settings.request_origins(false, 8000);

	// Assert
	assert_eq!(
		origins,
		[
			"https://reinhardt-cloud.dev",
			"https://www.reinhardt-cloud.dev"
		]
	);
}

#[rstest]
#[case::wildcard("*")]
#[case::wildcard_subdomain("https://*.example.test")]
#[case::empty("")]
#[case::not_http("ftp://example.test")]
#[case::no_scheme("example.test")]
#[case::userinfo("https://user@example.test")]
#[case::missing_host("https://")]
fn sr_12_a_wildcard_or_malformed_origin_is_ignored(#[case] entry: &str) {
	// Arrange
	let settings = settings("https://reinhardt-cloud.dev", entry);

	// Act
	let origins = settings.request_origins(false, 8000);

	// Assert
	assert_eq!(origins, ["https://reinhardt-cloud.dev"]);
}

#[rstest]
fn sr_12_a_path_or_query_on_an_origin_is_dropped() {
	// Arrange
	let settings = settings("", "https://a.example.test/path?q=1#frag");

	// Act
	let origins = settings.request_origins(false, 8000);

	// Assert
	assert_eq!(origins, ["https://a.example.test"]);
}

#[rstest]
fn sr_12_loopback_origins_exist_only_in_a_debug_profile() {
	// Arrange
	let settings = settings("https://reinhardt-cloud.dev", "");

	// Act
	let deployed = settings.request_origins(false, 8001);
	let debug = settings.request_origins(true, 8001);

	// Assert
	assert_eq!(deployed, ["https://reinhardt-cloud.dev"]);
	assert_eq!(
		debug,
		[
			"https://reinhardt-cloud.dev",
			"http://localhost:8001",
			"http://127.0.0.1:8001"
		]
	);
}

#[rstest]
fn the_github_app_needs_both_a_client_id_and_a_secret() {
	// Arrange
	let id_only = AccountsSettings {
		github_client_id: "Iv1.abc".to_owned(),
		..AccountsSettings::default()
	};
	let both = AccountsSettings {
		github_client_id: "Iv1.abc".to_owned(),
		github_client_secret: Some(SecretString::new("s3cret")),
		public_url: "https://reinhardt-cloud.dev/".to_owned(),
		..AccountsSettings::default()
	};

	// Act / Assert
	assert!(id_only.github_app().is_none());
	assert!(AccountsSettings::default().github_app().is_none());
	let app = both.github_app().unwrap();
	assert_eq!(app.client_id, "Iv1.abc");
	assert_eq!(
		app.redirect_uri,
		"https://reinhardt-cloud.dev/api/auth/github/callback/"
	);
	assert_eq!(
		app.authorize_url,
		"https://github.com/login/oauth/authorize"
	);
	assert_eq!(app.token_url, "https://github.com/login/oauth/access_token");
	assert_eq!(app.api_url, "https://api.github.com");
}

#[rstest]
fn the_github_app_debug_output_redacts_the_secret() {
	// Arrange
	let settings = AccountsSettings {
		github_client_id: "Iv1.abc".to_owned(),
		github_client_secret: Some(SecretString::new("super-secret-value")),
		..AccountsSettings::default()
	};

	// Act
	let rendered = format!("{:?}", settings.github_app().unwrap());

	// Assert
	assert!(!rendered.contains("super-secret-value"), "{rendered}");
}

#[rstest]
fn sr_13_trusted_proxies_are_exact_ip_addresses() {
	// Arrange
	let settings = AccountsSettings {
		trusted_proxies: "10.0.0.7, 2001:db8::1 ,".to_owned(),
		..AccountsSettings::default()
	};
	let malformed = AccountsSettings {
		trusted_proxies: "10.0.0.0/8".to_owned(),
		..AccountsSettings::default()
	};

	// Act
	let parsed = settings.trusted_proxy_addresses();
	let rejected = malformed.trusted_proxy_addresses();

	// Assert
	assert_eq!(
		parsed,
		Ok(vec![
			"10.0.0.7".parse().unwrap(),
			"2001:db8::1".parse().unwrap()
		])
	);
	assert_eq!(rejected, Err("10.0.0.0/8".to_owned()));
	assert_eq!(
		AccountsSettings::default().trusted_proxy_addresses(),
		Ok(vec![])
	);
}

#[rstest]
#[case::bare_origin("https://reinhardt-cloud.dev", Some("https://reinhardt-cloud.dev"))]
#[case::trailing_slash("https://reinhardt-cloud.dev/", Some("https://reinhardt-cloud.dev"))]
#[case::with_port("http://localhost:8000", Some("http://localhost:8000"))]
#[case::empty("", None)]
#[case::with_a_path("https://reinhardt-cloud.dev/dashboard", None)]
#[case::without_a_scheme("reinhardt-cloud.dev", None)]
fn the_public_origin_is_a_bare_origin_or_nothing(
	#[case] public_url: &str,
	#[case] expected: Option<&str>,
) {
	// Arrange
	let settings = settings(public_url, "");

	// Act
	let origin = settings.public_origin();

	// Assert
	assert_eq!(origin.as_deref(), expected);
}

#[rstest]
#[case::local(Profile::Development)]
#[case::staging(Profile::Staging)]
fn a_configured_github_app_without_a_public_origin_is_rejected_in_every_profile(
	#[case] profile: Profile,
) {
	// Arrange
	let settings = AccountsSettings {
		github_client_id: "Iv1.abc".to_owned(),
		github_client_secret: Some(SecretString::new("s3cret")),
		..AccountsSettings::default()
	};

	// Act
	let error = settings.validate(&profile).unwrap_err().to_string();

	// Assert
	assert_eq!(
		error,
		"Invalid value for 'accounts.public_url': must be a non-empty origin without a path (for example `https://host`) when the GitHub App is configured"
	);
}
