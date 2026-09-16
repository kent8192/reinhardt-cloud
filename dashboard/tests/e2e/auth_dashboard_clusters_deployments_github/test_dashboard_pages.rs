use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use reinhardt_test::{BrowserClient, BrowserConfig, browser_config};
use rstest::rstest;

use super::browser_session::BrowserSession;

const AUTHENTICATED_ROUTES: &[RouteExpectation] = &[
	RouteExpectation::new(
		"/",
		"h1",
		"Deployment Operations",
		"main h3",
		"Clusters",
		"home",
	),
	RouteExpectation::new("/account", "h1", "Account", "main h2", "Profile", "account"),
	RouteExpectation::new(
		"/clusters",
		"h1",
		"Clusters",
		"main h2",
		"Register Cluster",
		"clusters",
	),
	RouteExpectation::new(
		"/deployments",
		"h1",
		"Deployments",
		"main h2",
		"Create Deployment",
		"deployments",
	),
	RouteExpectation::new(
		"/github",
		"h1",
		"GitHub Repositories",
		"main h2",
		"Import",
		"github",
	),
];

const PUBLIC_ROUTES: &[RouteExpectation] = &[
	RouteExpectation::new(
		"/login",
		"h2",
		"Sign in to your account",
		"label:has(input[name='username']) > span",
		"Username",
		"login",
	),
	RouteExpectation::new(
		"/register",
		"h2",
		"Create your account",
		"label:has(input[name='email']) > span",
		"Email",
		"register",
	),
	RouteExpectation::new(
		"/dashboard-e2e-not-found",
		"h1",
		"404",
		"p",
		"Page not found",
		"not-found",
	),
];

const VIEWPORTS: &[Viewport] = &[
	Viewport::new("desktop", 1440, 1000),
	Viewport::new("mobile", 390, 844),
];

struct DashboardE2eConfig {
	base_url: String,
	username: String,
	password: String,
	screenshot_dir: PathBuf,
}

impl DashboardE2eConfig {
	fn from_env() -> Result<Self> {
		let screenshot_dir = PathBuf::from(required_env("DASHBOARD_E2E_SCREENSHOT_DIR")?);
		let screenshot_dir = screenshot_dir.canonicalize().with_context(|| {
			format!(
				"DASHBOARD_E2E_SCREENSHOT_DIR must name an existing directory: {}",
				screenshot_dir.display()
			)
		})?;
		let repository_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
			.parent()
			.context("Dashboard manifest must be inside the repository")?
			.canonicalize()
			.context("failed to resolve the Dashboard repository directory")?;
		ensure!(
			!screenshot_dir.starts_with(&repository_dir),
			"DASHBOARD_E2E_SCREENSHOT_DIR must be outside the repository"
		);

		Ok(Self {
			base_url: required_env("DASHBOARD_E2E_BASE_URL")?,
			username: required_env("DASHBOARD_E2E_USERNAME")?,
			password: required_env("DASHBOARD_E2E_PASSWORD")?,
			screenshot_dir,
		})
	}
}

struct RouteExpectation {
	path: &'static str,
	heading_selector: &'static str,
	heading: &'static str,
	content_selector: &'static str,
	content: &'static str,
	screenshot_stem: &'static str,
}

impl RouteExpectation {
	const fn new(
		path: &'static str,
		heading_selector: &'static str,
		heading: &'static str,
		content_selector: &'static str,
		content: &'static str,
		screenshot_stem: &'static str,
	) -> Self {
		Self {
			path,
			heading_selector,
			heading,
			content_selector,
			content,
			screenshot_stem,
		}
	}
}

struct Viewport {
	name: &'static str,
	width: u32,
	height: u32,
}

impl Viewport {
	const fn new(name: &'static str, width: u32, height: u32) -> Self {
		Self {
			name,
			width,
			height,
		}
	}
}

fn required_env(name: &str) -> Result<String> {
	let value = std::env::var(name).with_context(|| {
		format!("{name} must be set for the ignored Dashboard browser E2E test")
	})?;
	ensure!(
		!value.is_empty(),
		"{name} must not be empty for the ignored Dashboard browser E2E test"
	);
	Ok(value)
}

fn route_url(base_url: &str, path: &str) -> String {
	format!(
		"{}/{}",
		base_url.trim_end_matches('/'),
		path.trim_start_matches('/')
	)
}

async fn set_viewport(browser: &BrowserClient, viewport: &Viewport) -> Result<()> {
	browser
		.inner()
		.set_window_rect(0, 0, viewport.width, viewport.height)
		.await
		.with_context(|| {
			format!(
				"failed to set {} viewport to {}x{}",
				viewport.name, viewport.width, viewport.height
			)
		})
}

async fn login(browser: &BrowserClient, config: &DashboardE2eConfig) -> Result<()> {
	browser
		.navigate(&route_url(&config.base_url, "/login"))
		.await
		.context("failed to open the Dashboard login route")?;
	browser
		.wait_for("input[name='username']")
		.await
		.context("login username input did not appear")?;
	browser
		.wait_for("input[name='password']")
		.await
		.context("login password input did not appear")?;
	browser
		.type_into("input[name='username']", &config.username)
		.await
		.context("failed to enter the Dashboard E2E username")?;
	browser
		.type_into("input[name='password']", &config.password)
		.await
		.context("failed to enter the Dashboard E2E password")?;
	browser
		.click("button[type='submit']")
		.await
		.context("failed to submit the Dashboard login form")?;

	let home_url = route_url(&config.base_url, "/");
	browser
		.wait_for_url(|current| {
			current.as_str().trim_end_matches('/') == home_url.trim_end_matches('/')
		})
		.await
		.context("Dashboard login did not navigate to the authenticated home route")?;
	Ok(())
}

async fn verify_route(
	browser: &BrowserClient,
	config: &DashboardE2eConfig,
	viewport: &Viewport,
	route: &RouteExpectation,
) -> Result<()> {
	browser
		.navigate(&route_url(&config.base_url, route.path))
		.await
		.with_context(|| format!("failed to open Dashboard route {}", route.path))?;

	let heading = browser
		.wait_for(route.heading_selector)
		.await
		.with_context(|| format!("heading did not appear on Dashboard route {}", route.path))?
		.text()
		.await
		.with_context(|| format!("failed to read heading on Dashboard route {}", route.path))?;
	assert_eq!(
		heading, route.heading,
		"unexpected heading on Dashboard route {}",
		route.path
	);
	let content = browser
		.wait_for(route.content_selector)
		.await
		.with_context(|| format!("content did not appear on Dashboard route {}", route.path))?
		.text()
		.await
		.with_context(|| format!("failed to read content on Dashboard route {}", route.path))?;
	assert_eq!(
		content, route.content,
		"unexpected content on Dashboard route {}",
		route.path
	);

	browser
		.wait_for("link[rel='stylesheet'][href*='components']")
		.await
		.with_context(|| {
			format!(
				"components.css link did not appear on Dashboard route {}",
				route.path
			)
		})?;
	let stylesheet = browser
		.execute_js(
			r#"
			const link = Array.from(document.querySelectorAll("link[rel='stylesheet']"))
				.find((candidate) => {
					const pathname = new URL(candidate.href, document.baseURI).pathname;
					return pathname.includes("/__reinhardt__/components")
						&& pathname.endsWith(".css");
				});
			if (!link) return { pathname: null, ruleCount: 0 };
			let ruleCount = 0;
			try {
				ruleCount = link.sheet ? link.sheet.cssRules.length : 0;
			} catch (_) {}
			return {
				pathname: new URL(link.href, document.baseURI).pathname,
				ruleCount,
			};
			"#,
			vec![],
		)
		.await
		.with_context(|| {
			format!(
				"failed to inspect components.css on Dashboard route {}",
				route.path
			)
		})?;
	let stylesheet_path = stylesheet["pathname"].as_str().unwrap_or_default();
	let rule_count = stylesheet["ruleCount"].as_u64().unwrap_or_default();
	assert!(
		stylesheet_path.contains("/__reinhardt__/components") && stylesheet_path.ends_with(".css"),
		"Dashboard route {} must load the logical or hashed components stylesheet, got {:?}",
		route.path,
		stylesheet_path
	);
	assert!(
		rule_count > 0,
		"Dashboard route {} must load non-empty components.css",
		route.path
	);

	let viewport_widths = browser
		.execute_js(
			"return { scrollWidth: document.documentElement.scrollWidth, innerWidth: window.innerWidth };",
			vec![],
		)
		.await
		.with_context(|| format!("failed to measure Dashboard route {}", route.path))?;
	let scroll_width = viewport_widths["scrollWidth"].as_u64().unwrap_or(u64::MAX);
	let inner_width = viewport_widths["innerWidth"].as_u64().unwrap_or_default();
	assert!(
		scroll_width <= inner_width,
		"Dashboard route {} overflows {} viewport: scrollWidth={scroll_width}, innerWidth={inner_width}",
		route.path,
		viewport.name
	);

	let screenshot_path = config
		.screenshot_dir
		.join(format!("{}-{}.png", viewport.name, route.screenshot_stem));
	let screenshot = browser
		.screenshot()
		.await
		.with_context(|| format!("failed to capture Dashboard route {}", route.path))?;
	std::fs::write(&screenshot_path, screenshot)
		.with_context(|| format!("failed to write {}", screenshot_path.display()))?;
	Ok(())
}

async fn verify_public_route(
	browser_config: &BrowserConfig,
	config: &DashboardE2eConfig,
	viewport: &Viewport,
	route: &RouteExpectation,
) -> Result<()> {
	let browser = BrowserSession::connect(browser_config.clone())
		.await
		.with_context(|| format!("failed to create fresh browser for {}", route.path))?;
	let verification: Result<()> = async {
		set_viewport(&browser, viewport).await?;
		verify_route(&browser, config, viewport, route).await
	}
	.await;
	verification?;
	Ok(())
}

async fn verify_unauthenticated_redirect(
	browser_config: &BrowserConfig,
	config: &DashboardE2eConfig,
) -> Result<()> {
	let browser = BrowserSession::connect(browser_config.clone())
		.await
		.context("failed to create fresh browser for the unauthenticated route check")?;
	let verification: Result<()> = async {
		set_viewport(&browser, &VIEWPORTS[0]).await?;
		browser
			.navigate(&route_url(&config.base_url, "/clusters"))
			.await
			.context("failed to open the unauthenticated clusters route")?;
		let login_url = route_url(&config.base_url, "/login");
		browser
			.wait_for_url(|current| {
				current.as_str().trim_end_matches('/') == login_url.trim_end_matches('/')
			})
			.await
			.context("unauthenticated clusters route did not redirect to login")?;
		let heading = browser
			.wait_for("h2")
			.await
			.context("login heading did not appear after the unauthenticated redirect")?
			.text()
			.await
			.context("failed to read the redirected login heading")?;
		assert_eq!(heading, "Sign in to your account");
		Ok(())
	}
	.await;
	verification?;
	Ok(())
}

async fn run_dashboard_routes(browser: &BrowserClient, config: &DashboardE2eConfig) -> Result<()> {
	set_viewport(browser, &VIEWPORTS[0]).await?;
	login(browser, config).await?;

	for viewport in VIEWPORTS {
		set_viewport(browser, viewport).await?;
		for route in AUTHENTICATED_ROUTES {
			verify_route(browser, config, viewport, route).await?;
		}
	}

	let browser_config = browser.config().clone();
	for viewport in VIEWPORTS {
		for route in PUBLIC_ROUTES {
			verify_public_route(&browser_config, config, viewport, route).await?;
		}
	}
	verify_unauthenticated_redirect(&browser_config, config).await?;
	Ok(())
}

#[rstest]
#[tokio::test]
#[ignore = "requires caller-managed ChromeDriver, Dashboard, PostgreSQL, and Redis"]
async fn dashboard_routes_load_without_style_or_overflow_regressions(
	browser_config: BrowserConfig,
) -> Result<()> {
	let config = DashboardE2eConfig::from_env()?;
	let browser = BrowserSession::connect(browser_config).await?;
	let verification = run_dashboard_routes(&browser, &config).await;
	verification?;
	Ok(())
}

#[rstest]
#[case("http://localhost:8000", "/account", "http://localhost:8000/account")]
#[case("http://localhost:8000/", "clusters", "http://localhost:8000/clusters")]
fn route_url_joins_one_separator(
	#[case] base_url: &str,
	#[case] path: &str,
	#[case] expected: &str,
) {
	assert_eq!(route_url(base_url, path), expected);
}
