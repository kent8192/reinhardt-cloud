//! The request surface over real HTTP: access control, the enumerated
//! unauthenticated surface, API documentation, cross-site protection, headers,
//! and error hygiene (SR-01, SR-07, SR-09 to SR-14, SR-20).

use reinhardt::db::orm::Model;
use rstest::rstest;
use serde_json::json;
use serial_test::serial;

use crate::apps::accounts::models::User;
use crate::apps::accounts::tests::server_support::{
	AppOptions, Browser, GithubAccount, Reply, TestApp,
};
use crate::apps::accounts::tests::support::insert_user;
use crate::apps::accounts::urls::server_router::ANONYMOUS_PATHS;

const CURRENT_VIEWER: &str = "/api/server_fn/current_viewer";

/// Sign `account` in and return the browser holding the session.
async fn signed_in(app: &TestApp, account: &GithubAccount, code: &str) -> Browser {
	app.expect_sign_in(code, account).await;
	let mut browser = app.browser();
	let callback = browser.sign_in(code).await;
	assert_eq!(callback.header("location"), Some("/"), "{callback:?}");
	browser
}

/// The admin site's own policy, exactly as `reinhardt-admin` composes it.
const ADMIN_CSP: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'";

async fn admin_dashboard(browser: &mut Browser, origin: &str) -> Reply {
	browser
		.post_with(
			"/admin/api/server_fn/get_dashboard",
			json!({}),
			&[("Origin", origin)],
		)
		.await
}

async fn set_staff(github_user_id: i64, value: bool) {
	User::objects()
		.filter(User::field_github_user_id().eq(github_user_id))
		.update_fields([User::field_is_staff().assign(value)])
		.await
		.unwrap();
}

/// The body the enumeration sends to `path`. Every route takes `{}` except the
/// one that needs its argument to get as far as answering: confirming a Login
/// Link with an empty token is a well-formed request that signs nobody in.
fn anonymous_request_body(path: &str) -> serde_json::Value {
	if path == "/api/server_fn/consume_login_link" {
		json!({"token": ""})
	} else {
		json!({})
	}
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_10_only_the_enumerated_routes_answer_an_anonymous_request() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let router = reinhardt::get_router().expect("the server registered its router");
	let mut browser = app.browser();
	let mut answered = Vec::new();
	let mut refused = Vec::new();
	let mut enumerated = 0;

	// Act
	for (path, _name, _namespace, methods) in router.get_all_routes() {
		let concrete = path.replace("{*tail}", "x").replace("{*path}", "x/y.css");
		for method in methods {
			enumerated += 1;
			let reply = browser
				.request_with_body(
					method.as_str(),
					&concrete,
					&[("Origin", &app.base_url)],
					&anonymous_request_body(&concrete),
				)
				.await;
			let success = (200..400).contains(&reply.status);
			if success {
				answered.push(format!("{} {path}", method.as_str()));
			} else {
				refused.push((format!("{} {path}", method.as_str()), reply.status));
			}
		}
	}

	// Assert
	let mut expected: Vec<String> = ANONYMOUS_PATHS
		.iter()
		.map(|path| {
			let method = if path.starts_with("/api/auth/") {
				"GET"
			} else {
				"POST"
			};
			format!("{method} {path}")
		})
		.collect();
	expected.sort();
	answered.sort();
	assert_eq!(
		answered, expected,
		"a route outside the enumerated list answered an anonymous request"
	);
	assert!(
		refused
			.iter()
			.all(|(_, status)| matches!(status, 401 | 403 | 404)),
		"{refused:?}"
	);
	assert_eq!(
		refused.len(),
		enumerated - ANONYMOUS_PATHS.len(),
		"every other enumerated route, the admin routes included, was refused: {refused:?}"
	);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_01_no_route_accepts_a_credential() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let router = reinhardt::get_router().expect("the server registered its router");
	let staff = insert_user(6_001, "ops", true).await;
	let mut account = GithubAccount::new(staff.github_user_id, "ops");
	account.name = None;
	let mut staff_browser = signed_in(&app, &account, "code-staff").await;
	let mut anonymous = app.browser();

	// Act
	let named_like_a_credential: Vec<String> = router
		.get_all_routes()
		.into_iter()
		.map(|(path, ..)| path)
		.filter(|path| {
			["login", "password", "register", "signup", "reset", "verify"]
				.iter()
				.any(|word| path.to_ascii_lowercase().contains(word))
		})
		.collect();
	// Confirming a Login Link takes a single-use grant a host operator issued
	// (SR-16, SR-18), not a username or password; it is checked on its own below.
	let (confirmation, suspicious): (Vec<_>, Vec<_>) = named_like_a_credential
		.into_iter()
		.partition(|path| path == "/api/server_fn/consume_login_link");
	let mut answers = Vec::new();
	for path in &suspicious {
		let body = json!({"username": "ops", "password": "hunter2"});
		let by_anonymous = anonymous
			.post_with(path, body.clone(), &[("Origin", &app.base_url)])
			.await;
		let by_staff = staff_browser
			.post_with(path, body, &[("Origin", &app.base_url)])
			.await;
		answers.push((path.clone(), by_anonymous.status, by_staff.status));
	}

	// Assert
	assert_eq!(
		suspicious,
		[
			"/admin/api/server_fn/admin_login",
			"/admin/api/server_fn/admin_login_with_header"
		],
		"only the admin site's built-in credential endpoints are named like one"
	);
	assert_eq!(confirmation, ["/api/server_fn/consume_login_link"]);
	let password_attempt = anonymous
		.post_with(
			&confirmation[0],
			json!({"username": "ops", "password": "hunter2", "token": "hunter2"}),
			&[("Origin", &app.base_url)],
		)
		.await;
	assert_eq!(password_attempt.json(), json!("Rejected"));
	assert!(password_attempt.set_cookie("cloud_session").is_none());
	assert!(
		answers
			.iter()
			.all(|(_, anonymous, staff)| (*anonymous, *staff) == (404, 404)),
		"{answers:?}"
	);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_09_private_endpoints_reject_anonymous_callers() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let mut browser = app.browser();

	// Act
	let private_api = browser.get("/api/anything-private/").await;
	let admin_api = browser
		.post_with(
			"/admin/api/server_fn/get_list",
			json!({}),
			&[("Origin", &app.base_url)],
		)
		.await;
	let admin_json = browser.get("/admin/").await;
	let admin_page = browser
		.get_with("/admin/", &[("Accept", "text/html,application/xhtml+xml")])
		.await;

	// Assert
	assert_eq!(private_api.status, 401);
	assert_eq!(
		private_api.json(),
		json!({"error": "authentication required"})
	);
	assert_eq!(admin_api.status, 401);
	assert_eq!(admin_json.status, 401);
	assert_eq!(admin_page.status, 302);
	assert_eq!(admin_page.header("location"), Some("/sign-in/"));
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_20_the_admin_site_is_reachable_only_by_active_staff_and_follows_the_database() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let account = GithubAccount::new(6_002, "operator");
	let mut browser = signed_in(&app, &account, "code-op").await;
	// Act / Assert
	let not_staff = browser.get("/admin/").await;
	assert_eq!(not_staff.status, 403, "a signed-in User who is not Staff");
	assert_eq!(
		admin_dashboard(&mut browser, &app.base_url).await.status,
		403
	);

	set_staff(6_002, true).await;
	let staff_shell = browser.get("/admin/").await;
	assert_eq!(
		staff_shell.status, 200,
		"Staff granted: effective on the next request"
	);
	let staff_api = admin_dashboard(&mut browser, &app.base_url).await;
	assert!(
		!matches!(staff_api.status, 401 | 403 | 404),
		"the admin loader accepts active Staff: {staff_api:?}"
	);
	assert_eq!(
		staff_shell.header("content-security-policy"),
		Some(ADMIN_CSP),
		"the admin site keeps its own policy"
	);

	set_staff(6_002, false).await;
	assert_eq!(
		browser.get("/admin/").await.status,
		403,
		"Staff revoked: effective at once"
	);
	assert_eq!(
		admin_dashboard(&mut browser, &app.base_url).await.status,
		403
	);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_07_staff_status_applies_to_the_next_request_without_signing_in_again() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let account = GithubAccount::new(6_003, "promoted");
	let mut browser = signed_in(&app, &account, "code-promoted").await;
	let before = browser
		.post_json(CURRENT_VIEWER, json!({}), Some(&app.base_url))
		.await;

	// Act
	set_staff(6_003, true).await;
	let after = browser
		.post_json(CURRENT_VIEWER, json!({}), Some(&app.base_url))
		.await;

	// Assert
	assert_eq!(before.json()["is_staff"], false);
	assert_eq!(after.json()["is_staff"], true);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_11_documentation_is_served_in_the_ci_profile_without_the_swagger_ui_feature() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let mut browser = app.browser();

	// Act
	let schema = browser.get("/api/openapi.json").await;
	let swagger = browser.get("/api/docs").await;
	let redoc = browser.get("/api/redoc").await;

	// Assert
	assert_eq!(schema.status, 200);
	assert_eq!(
		schema.json()["openapi"]
			.as_str()
			.map(|v| v.starts_with("3.")),
		Some(true)
	);
	assert_eq!(swagger.status, 200);
	assert!(swagger.body.to_ascii_lowercase().contains("swagger"));
	assert_eq!(redoc.status, 200);
	assert!(redoc.body.to_ascii_lowercase().contains("redoc"));
	let secrets = ["Iv1.test-client-id", "test-github-client-secret"];
	for secret in secrets {
		assert!(
			!schema.body.contains(secret),
			"the document must hold no secret"
		);
	}
}

#[rstest]
#[case::staging("staging")]
#[case::production("production")]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_11_documentation_is_not_registered_in_deployed_profiles(#[case] profile: &'static str) {
	// Arrange
	let app = TestApp::start(AppOptions {
		profile,
		github_configured: false,
		..AppOptions::default()
	})
	.await;
	let mut browser = app.browser();

	// Act
	let mut documentation = Vec::new();
	for path in ["/api/openapi.json", "/api/docs", "/api/redoc"] {
		documentation.push(browser.get(path).await);
	}
	let unknown = browser.get("/api/never-existed").await;

	// Assert
	for reply in &documentation {
		assert_eq!(reply.status, unknown.status);
		assert_eq!(reply.body, unknown.body);
		assert!(!reply.body.to_ascii_lowercase().contains("swagger"));
	}
	assert_eq!(unknown.status, 401);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_12_a_cookie_authenticated_write_from_another_site_is_rejected_and_harms_nothing() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let account = GithubAccount::new(6_004, "careful");
	let mut browser = signed_in(&app, &account, "code-careful").await;
	let sign_out = "/api/server_fn/sign_out";

	// Act
	let no_origin = browser.post_with(sign_out, json!({}), &[]).await;
	let evil = browser
		.post_with(sign_out, json!({}), &[("Origin", "https://evil.example")])
		.await;
	let null = browser
		.post_with(sign_out, json!({}), &[("Origin", "null")])
		.await;
	let still_signed_in = browser
		.post_json(CURRENT_VIEWER, json!({}), Some(&app.base_url))
		.await;
	let referer_only = browser
		.post_with(
			sign_out,
			json!({}),
			&[("Referer", &format!("{}/somewhere/", app.base_url))],
		)
		.await;

	// Assert
	for rejected in [&no_origin, &evil, &null] {
		assert_eq!(rejected.status, 403, "{rejected:?}");
		assert_eq!(
			rejected.json(),
			json!({"error": "cross-site request rejected"})
		);
	}
	assert_eq!(
		still_signed_in.json()["github_login"],
		"careful",
		"rejected requests change nothing"
	);
	assert_eq!(
		referer_only.status, 200,
		"a Referer from the allowed origin proves intent too"
	);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_12_a_bearer_header_does_not_exempt_a_cookie_request_from_the_origin_rule() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let account = GithubAccount::new(6_005, "scripted");
	let mut browser = signed_in(&app, &account, "code-scripted").await;

	// Act
	let reply = browser
		.post_with(
			CURRENT_VIEWER,
			json!({}),
			&[("Authorization", "Bearer cli-session-token")],
		)
		.await;

	// Assert
	// Bearer CLI Sessions arrive with M2; until then an `Authorization` header
	// is no proof of same-origin intent for a request that carries the cookie.
	assert_eq!(reply.status, 403, "{reply:?}");
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_13_api_responses_carry_the_security_headers_on_success_and_on_error() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let mut browser = app.browser();

	// Act
	let ok = browser.post_json(CURRENT_VIEWER, json!({}), None).await;
	let unauthorized = browser.get("/api/private/").await;
	let rejected = {
		browser.set_cookie("cloud_session", "x");
		browser
			.post_with("/api/server_fn/sign_out", json!({}), &[])
			.await
	};

	// Assert
	assert_eq!(
		(ok.status, unauthorized.status, rejected.status),
		(200, 401, 403)
	);
	for reply in [&ok, &unauthorized, &rejected] {
		assert_eq!(
			reply.header("content-security-policy"),
			Some("default-src 'none'; frame-ancestors 'none'"),
			"{reply:?}"
		);
		assert_eq!(reply.header("x-content-type-options"), Some("nosniff"));
		assert_eq!(reply.header("x-frame-options"), Some("DENY"));
		assert_eq!(reply.header("referrer-policy"), Some("same-origin"));
		assert_eq!(reply.header("cache-control"), Some("no-store"));
		assert_eq!(
			reply.header("strict-transport-security"),
			None,
			"plain HTTP"
		);
	}
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_14_a_storage_outage_reaches_the_client_as_a_generic_error() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let mut browser = app.browser();
	app.stop_redis().await;
	browser.set_cookie("cloud_signin_notice", "any-notice-id");

	// Act
	let notice = browser
		.post_json(
			"/api/server_fn/take_sign_in_notice",
			json!({}),
			Some(&app.base_url),
		)
		.await;
	let start = browser.get("/api/auth/github/").await;

	// Assert
	assert_eq!(notice.status, 500, "{notice:?}");
	assert_eq!(start.status, 302);
	assert_eq!(start.header("location"), Some("/sign-in/"));
	for reply in [&notice, &start] {
		let everything = format!("{} {:?}", reply.body, reply.headers).to_ascii_lowercase();
		for leaked in [
			"redis",
			"connection refused",
			"127.0.0.1",
			"localhost",
			"tcp",
			"os error",
		] {
			assert!(
				!everything
					.replace(&app.base_url.to_ascii_lowercase(), "")
					.contains(leaked),
				"the response leaks `{leaked}`: {everything}"
			);
		}
	}
	assert_eq!(
		notice.json(),
		json!({"version": 1, "kind": "server", "status": 500, "message": "Internal server error", "field_errors": []}),
		"the server function failed with its fixed generic message"
	);
}

/// Documents the SR-13 gap on the single-page application shell.
///
/// Workaround for kent8192/reinhardt-web#6721 (tracked in
/// kent8192/reinhardt-cloud#949): `runserver --with-pages` answers the shell
/// and static assets from a static layer in front of the router, so they carry
/// none of the security headers the router adds. Serving the shell from a
/// router catch-all (`--no-spa`) was tried: it does get `PAGE_CSP` and the frame
/// protections, but the document would then have to be rendered by the
/// application, including the WASM loader the static layer injects (an inline
/// module script in the development build, which `script-src 'self'` blocks,
/// and a manifest-driven entry in the production build, which the application
/// cannot reproduce without the framework's private asset code).
///
/// This test fails when upstream starts attaching headers to the shell, which
/// is the signal to delete this workaround and assert `PAGE_CSP` instead.
///
/// Ideal implementation (without workaround):
///   `assert_eq!(shell.header("content-security-policy"), Some(PAGE_CSP));`
///   // The shell document carries the page policy and frame protections.
#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_13_the_spa_shell_currently_carries_no_security_headers_upstream_gap() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let mut browser = app.browser();

	// Act
	let shell = browser.get("/sign-in/").await;

	// Assert
	assert_eq!(shell.status, 200);
	assert_eq!(shell.header("content-type"), Some("text/html"));
	for missing in [
		"content-security-policy",
		"x-frame-options",
		"x-content-type-options",
		"referrer-policy",
	] {
		assert_eq!(
			shell.header(missing),
			None,
			"upstream now sets `{missing}` on the shell: remove the workaround (reinhardt-web#6721)"
		);
	}
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_18_20_107_no_route_issues_a_link_grants_staff_or_moves_a_user() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let router = reinhardt::get_router().expect("the server registered its router");

	// Act
	let paths: Vec<String> = router
		.get_all_routes()
		.into_iter()
		.map(|(path, ..)| path.to_ascii_lowercase())
		.collect();
	let mentioning = |words: &[&str]| -> Vec<String> {
		paths
			.iter()
			.filter(|path| words.iter().any(|word| path.contains(word)))
			.cloned()
			.collect()
	};

	// Assert
	assert!(
		mentioning(&["staff", "grant", "repoint", "github_user_id", "issue"]).is_empty(),
		"Staff grants, Login Link issuance, and re-pointing exist only as `manage` commands: {paths:?}"
	);
	assert_eq!(
		mentioning(&["login_link", "login-link", "loginlink"]),
		["/api/server_fn/consume_login_link"],
		"the only Login Link route confirms an existing link"
	);
	drop(app);
}
