//! A real Control Plane server for the sign-in and request-surface tests.
//!
//! [`TestApp::start`] brings up PostgreSQL (with the committed migrations) and
//! Redis in containers, a local stand-in for GitHub, and the production server
//! entry point (`server::serve`) on a free loopback port, configured only
//! through the same environment variables a deployment uses. [`Browser`] talks
//! to it over real HTTP, keeping cookies the way a browser would. GitHub is
//! never called.

use std::collections::BTreeMap;
use std::time::Duration;

use reinhardt::test::fixtures::{ContainerAsync, GenericImage, redis_container};
use reqwest::header::HeaderMap;
use serde_json::{Value, json};
use tokio::task::JoinHandle;
use wiremock::matchers::{body_string_contains, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::apps::accounts::tests::support::{TestDatabase, database};
use crate::config::settings::test_support::{EnvGuard, required_env};
use crate::server::{prepare_context, serve};

/// How a test configures the server under test.
#[derive(Clone, Debug)]
pub(crate) struct AppOptions {
	/// `REINHARDT_ENV`.
	pub profile: &'static str,
	/// `REINHARDT_CLOUD_SIGN_UP_POLICY`.
	pub sign_up_policy: &'static str,
	/// `REINHARDT_CLOUD_SIGN_UP_ALLOWED_GITHUB_USER_IDS`.
	pub allowed_user_ids: &'static str,
	/// `REINHARDT_CLOUD_SIGN_UP_ALLOWED_GITHUB_ORGANIZATION_IDS`.
	pub allowed_organization_ids: &'static str,
	/// Whether the GitHub App credentials are set.
	pub github_configured: bool,
}

impl Default for AppOptions {
	fn default() -> Self {
		Self {
			profile: "ci",
			sign_up_policy: "open",
			allowed_user_ids: "",
			allowed_organization_ids: "",
			github_configured: true,
		}
	}
}

/// A GitHub account the stand-in knows.
#[derive(Clone, Debug)]
pub(crate) struct GithubAccount {
	pub id: i64,
	pub login: &'static str,
	pub name: Option<&'static str>,
	pub organization_ids: Vec<i64>,
	pub access_token: String,
	pub refresh_token: String,
}

impl GithubAccount {
	pub(crate) fn new(id: i64, login: &'static str) -> Self {
		Self {
			id,
			login,
			name: None,
			organization_ids: Vec::new(),
			access_token: format!("ghu_access_{id}"),
			refresh_token: format!("ghr_refresh_{id}"),
		}
	}
}

/// The running server and everything around it.
pub(crate) struct TestApp {
	pub(crate) base_url: String,
	pub(crate) github: MockServer,
	/// URL of the Redis the server uses, for tests that tamper with its data.
	pub(crate) redis_url: String,
	server: JoinHandle<()>,
	_database: TestDatabase,
	_redis: ContainerAsync<GenericImage>,
	_env: EnvGuard,
}

impl Drop for TestApp {
	fn drop(&mut self) {
		self.server.abort();
	}
}

impl TestApp {
	/// Start everything. The caller must hold `#[serial(database, env_settings_load)]`.
	pub(crate) async fn start(options: AppOptions) -> Self {
		let db = database().await;
		let (redis, _redis_port, redis_url) = redis_container().await;
		let github = MockServer::start().await;
		let database_port = db.port().await;
		let port = free_port();
		let base_url = format!("http://127.0.0.1:{port}");

		let github_uri = github.uri();
		let authorize_url = format!("{github_uri}/login/oauth/authorize");
		let token_url = format!("{github_uri}/login/oauth/access_token");
		let database_port = database_port.to_string();
		let mut vars = required_env(options.profile);
		vars.push(("REINHARDT_DATABASE_HOST", Some("localhost")));
		vars.push(("REINHARDT_DATABASE_PORT", Some(database_port.as_str())));
		vars.push(("REINHARDT_DATABASE_NAME", Some("postgres")));
		vars.push(("REINHARDT_DATABASE_USER", Some("postgres")));
		vars.push(("REINHARDT_CLOUD_REDIS_URL", Some(redis_url.as_str())));
		vars.push(("REINHARDT_CLOUD_PUBLIC_URL", Some(base_url.as_str())));
		vars.push((
			"REINHARDT_CLOUD_SIGN_UP_POLICY",
			Some(options.sign_up_policy),
		));
		vars.push((
			"REINHARDT_CLOUD_SIGN_UP_ALLOWED_GITHUB_USER_IDS",
			Some(options.allowed_user_ids),
		));
		vars.push((
			"REINHARDT_CLOUD_SIGN_UP_ALLOWED_GITHUB_ORGANIZATION_IDS",
			Some(options.allowed_organization_ids),
		));
		let (client_id, client_secret) = if options.github_configured {
			(
				Some("Iv1.test-client-id"),
				Some("test-github-client-secret"),
			)
		} else {
			(Some(""), Some(""))
		};
		// A deployed profile starts without the GitHub App only on an explicit
		// opt-out.
		vars.push((
			"REINHARDT_CLOUD_GITHUB_SIGN_IN",
			(!options.github_configured).then_some("disabled"),
		));
		vars.push(("REINHARDT_CLOUD_GITHUB_CLIENT_ID", client_id));
		vars.push(("REINHARDT_CLOUD_GITHUB_CLIENT_SECRET", client_secret));
		vars.push((
			"REINHARDT_CLOUD_GITHUB_AUTHORIZE_URL",
			Some(authorize_url.as_str()),
		));
		vars.push(("REINHARDT_CLOUD_GITHUB_TOKEN_URL", Some(token_url.as_str())));
		vars.push(("REINHARDT_CLOUD_GITHUB_API_URL", Some(github_uri.as_str())));
		let env = EnvGuard::apply(&vars);

		let context = prepare_context(&format!("127.0.0.1:{port}"))
			.expect("the test settings should be valid");
		let server = tokio::spawn(async move {
			let _ = serve(&context).await;
		});
		wait_until_listening(&base_url).await;

		Self {
			base_url,
			github,
			redis_url,
			server,
			_database: db,
			_redis: redis,
			_env: env,
		}
	}

	/// Stop Redis, as an outage would.
	pub(crate) async fn stop_redis(&self) {
		self._redis.stop().await.expect("Redis stops");
	}

	pub(crate) fn browser(&self) -> Browser {
		Browser::new(&self.base_url)
	}

	/// Teach the GitHub stand-in to complete a sign-in with `code` for `account`.
	pub(crate) async fn expect_sign_in(&self, code: &str, account: &GithubAccount) {
		Mock::given(method("POST"))
			.and(path("/login/oauth/access_token"))
			.and(body_string_contains("grant_type=authorization_code"))
			.and(body_string_contains(format!("code={code}")))
			.and(body_string_contains("code_verifier="))
			.and(body_string_contains("client_id=Iv1.test-client-id"))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!({
				"access_token": account.access_token,
				"token_type": "bearer",
				"expires_in": 28800,
				"refresh_token": account.refresh_token,
				"refresh_token_expires_in": 15811200,
				"scope": ""
			})))
			.mount(&self.github)
			.await;
		self.expect_profile(account).await;
	}

	/// Teach the stand-in the profile and organizations behind `account`'s token.
	pub(crate) async fn expect_profile(&self, account: &GithubAccount) {
		let bearer = format!("Bearer {}", account.access_token);
		Mock::given(method("GET"))
			.and(path("/user"))
			.and(header("authorization", bearer.as_str()))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!({
				"id": account.id,
				"login": account.login,
				"name": account.name,
				"avatar_url": format!("https://avatars.example.test/u/{}", account.id),
				"email": null
			})))
			.mount(&self.github)
			.await;
		let organizations: Vec<Value> = account
			.organization_ids
			.iter()
			.map(|id| json!({"id": id, "login": format!("org-{id}")}))
			.collect();
		Mock::given(method("GET"))
			.and(path("/user/orgs"))
			.and(header("authorization", bearer.as_str()))
			.and(query_param("page", "1"))
			.respond_with(ResponseTemplate::new(200).set_body_json(organizations))
			.mount(&self.github)
			.await;
	}
}

fn free_port() -> u16 {
	let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port is free");
	listener
		.local_addr()
		.expect("the listener has an address")
		.port()
}

async fn wait_until_listening(base_url: &str) {
	let client = reqwest::Client::new();
	for _ in 0..300 {
		if client
			.post(format!("{base_url}/api/server_fn/current_viewer"))
			.send()
			.await
			.is_ok()
		{
			return;
		}
		tokio::time::sleep(Duration::from_millis(100)).await;
	}
	panic!("the server did not start listening on {base_url}");
}

/// What the server answered.
#[derive(Debug)]
pub(crate) struct Reply {
	pub status: u16,
	pub headers: HeaderMap,
	pub body: String,
}

impl Reply {
	pub(crate) fn header(&self, name: &str) -> Option<&str> {
		self.headers.get(name).and_then(|value| value.to_str().ok())
	}

	pub(crate) fn json(&self) -> Value {
		serde_json::from_str(&self.body).unwrap_or(Value::Null)
	}

	/// Every `Set-Cookie` line.
	pub(crate) fn set_cookies(&self) -> Vec<String> {
		self.headers
			.get_all("set-cookie")
			.iter()
			.filter_map(|value| value.to_str().ok().map(str::to_owned))
			.collect()
	}

	/// The `Set-Cookie` line of cookie `name`.
	pub(crate) fn set_cookie(&self, name: &str) -> Option<String> {
		self.set_cookies()
			.into_iter()
			.find(|line| line.starts_with(&format!("{name}=")))
	}
}

/// An HTTP client that keeps cookies like a browser and never follows redirects.
pub(crate) struct Browser {
	http: reqwest::Client,
	base_url: String,
	jar: BTreeMap<String, String>,
}

impl Browser {
	fn new(base_url: &str) -> Self {
		Self {
			http: reqwest::Client::builder()
				.redirect(reqwest::redirect::Policy::none())
				.timeout(Duration::from_secs(30))
				.build()
				.expect("the test client builds"),
			base_url: base_url.to_owned(),
			jar: BTreeMap::new(),
		}
	}

	pub(crate) fn cookie(&self, name: &str) -> Option<&str> {
		self.jar.get(name).map(String::as_str)
	}

	pub(crate) fn set_cookie(&mut self, name: &str, value: &str) {
		self.jar.insert(name.to_owned(), value.to_owned());
	}

	fn cookie_header(&self) -> Option<String> {
		(!self.jar.is_empty()).then(|| {
			self.jar
				.iter()
				.map(|(name, value)| format!("{name}={value}"))
				.collect::<Vec<_>>()
				.join("; ")
		})
	}

	async fn send(&mut self, request: reqwest::RequestBuilder) -> Reply {
		let request = match self.cookie_header() {
			Some(cookies) => request.header("Cookie", cookies),
			None => request,
		};
		let response = request.send().await.expect("the server answers");
		let status = response.status().as_u16();
		let headers = response.headers().clone();
		let body = response.text().await.unwrap_or_default();
		let reply = Reply {
			status,
			headers,
			body,
		};
		for line in reply.set_cookies() {
			self.absorb(&line);
		}
		reply
	}

	/// Update the jar from one `Set-Cookie` line.
	fn absorb(&mut self, line: &str) {
		let mut parts = line.split(';').map(str::trim);
		let Some((name, value)) = parts.next().and_then(|pair| pair.split_once('=')) else {
			return;
		};
		let expired = parts.any(|attribute| attribute.eq_ignore_ascii_case("max-age=0"));
		if expired || value.is_empty() {
			self.jar.remove(name);
		} else {
			self.jar.insert(name.to_owned(), value.to_owned());
		}
	}

	fn url(&self, path_or_url: &str) -> String {
		if path_or_url.starts_with("http://") || path_or_url.starts_with("https://") {
			path_or_url.to_owned()
		} else {
			format!("{}{path_or_url}", self.base_url)
		}
	}

	pub(crate) async fn get(&mut self, path_or_url: &str) -> Reply {
		let request = self.http.get(self.url(path_or_url));
		self.send(request).await
	}

	pub(crate) async fn get_with(&mut self, path: &str, headers: &[(&str, &str)]) -> Reply {
		let mut request = self.http.get(self.url(path));
		for (name, value) in headers {
			request = request.header(*name, *value);
		}
		self.send(request).await
	}

	/// POST `body` as JSON, the way a server function is called. `origin` is
	/// sent as the `Origin` header when given.
	pub(crate) async fn post_json(
		&mut self,
		path: &str,
		body: Value,
		origin: Option<&str>,
	) -> Reply {
		let mut request = self.http.post(self.url(path)).json(&body);
		if let Some(origin) = origin {
			request = request.header("Origin", origin);
		}
		self.send(request).await
	}

	/// POST `body` as JSON with exactly the given extra headers.
	pub(crate) async fn post_with(
		&mut self,
		path: &str,
		body: Value,
		headers: &[(&str, &str)],
	) -> Reply {
		let mut request = self.http.post(self.url(path)).json(&body);
		for (name, value) in headers {
			request = request.header(*name, *value);
		}
		self.send(request).await
	}

	/// Send a request with any method, JSON body, and headers.
	pub(crate) async fn request(
		&mut self,
		method: &str,
		path: &str,
		headers: &[(&str, &str)],
	) -> Reply {
		let method = reqwest::Method::from_bytes(method.as_bytes()).expect("a valid method");
		let mut request = self.http.request(method, self.url(path)).json(&json!({}));
		for (name, value) in headers {
			request = request.header(*name, *value);
		}
		self.send(request).await
	}

	/// Run the whole GitHub sign-in as the signed-out visitor would: start,
	/// follow GitHub's authorization redirect, and return from GitHub with `code`.
	/// Returns the callback's reply.
	pub(crate) async fn sign_in(&mut self, code: &str) -> Reply {
		let start = self.get("/api/auth/github/").await;
		assert_eq!(start.status, 302, "starting a sign-in redirects: {start:?}");
		let location = start.header("location").expect("a redirect has a target");
		let state = query_value(location, "state").expect("the redirect carries the state");
		self.get(&format!(
			"/api/auth/github/callback/?code={code}&state={state}"
		))
		.await
	}
}

/// The value of query parameter `name` in `url`.
pub(crate) fn query_value(url: &str, name: &str) -> Option<String> {
	let query = url.split_once('?')?.1;
	query
		.split('&')
		.filter_map(|pair| pair.split_once('='))
		.find(|(key, _)| *key == name)
		.map(|(_, value)| value.to_owned())
}
