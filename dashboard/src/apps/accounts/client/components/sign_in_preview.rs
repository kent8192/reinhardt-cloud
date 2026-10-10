//! The decorative product preview beside the sign-in form (Variant A).
//!
//! It shows what the Control Plane does: a Deployment applying to a Cluster,
//! with its phases and a live log. Every name, time, and line below is sample
//! content for the picture; none of it is data. The sign-in layout hides the
//! whole aside from assistive technology and makes it inert, so nothing here
//! carries information or controls.
//!
//! `reinhardt-pages` checked: `ui` and `tables` have nothing for a phase
//! strip or a log excerpt, so these are plain elements styled with
//! `#[style_def]`; the status marks reuse the shared `badge` and `chip`.

use reinhardt::pages::component::{IntoPage, Page};
use reinhardt::pages::{page, t};

use crate::apps::accounts::client::style::STYLES;
use crate::components::badge::{BadgeStatus, badge};

// Sample content: names and clock times are not translated, sentences are.
const PROJECT: &str = "storefront-api";
const CLUSTER: &str = "prod-tokyo";
const SUBMITTER: &str = "priya-raman";
const COMMIT: &str = "a3f91c2";
const DEPLOYMENT_NUMBER: &str = "148";

/// One row of the log excerpt: clock time, level, and message.
struct LogLine {
	time: &'static str,
	warn: bool,
	message: Page,
}

fn log_lines() -> Vec<LogLine> {
	let line = |time, warn, message| LogLine {
		time,
		warn,
		message,
	};
	vec![
		line(
			"14:28:09",
			false,
			IntoPage::into_page(t!(
				"applying Project {project} generation {number}",
				project = PROJECT,
				number = DEPLOYMENT_NUMBER
			)),
		),
		line(
			"14:28:12",
			false,
			IntoPage::into_page(t!("migrate: {count} pending migrations", count = "2")),
		),
		line("14:28:17", false, IntoPage::into_page(t!("migrate: done"))),
		line(
			"14:28:19",
			false,
			IntoPage::into_page(t!("replicas: starting {count} replicas", count = "3")),
		),
		line(
			"14:28:24",
			false,
			IntoPage::into_page(t!("{replica} listening on 0.0.0.0:8000", replica = "x2k4q")),
		),
		line(
			"14:28:25",
			false,
			IntoPage::into_page(t!("{replica} readiness probe passed", replica = "x2k4q")),
		),
		line(
			"14:28:31",
			false,
			IntoPage::into_page(t!("{replica} listening on 0.0.0.0:8000", replica = "m8t7w")),
		),
		line(
			"14:28:32",
			true,
			IntoPage::into_page(t!(
				"{replica} readiness probe failed (attempt {attempt} of {attempts})",
				replica = "m8t7w",
				attempt = "1",
				attempts = "3"
			)),
		),
		line(
			"14:28:36",
			false,
			IntoPage::into_page(t!("{replica} readiness probe passed", replica = "m8t7w")),
		),
		line(
			"14:28:41",
			false,
			IntoPage::into_page(t!(
				"replicas: {ready} of {total} replicas ready",
				ready = "2",
				total = "3"
			)),
		),
		line(
			"14:28:47",
			false,
			IntoPage::into_page(t!("{replica} listening on 0.0.0.0:8000", replica = "q5r2d")),
		),
		line(
			"14:28:51",
			false,
			IntoPage::into_page(t!("{replica} cache warm-up: ok", replica = "q5r2d")),
		),
	]
}

fn phase(done: bool, current: bool, name: Page, time: Page) -> Page {
	let classes = match (done, current) {
		(true, _) => STYLES.phase() + STYLES.phase_done(),
		(false, true) => STYLES.phase() + STYLES.phase_current() + "rc-progress-wash",
		(false, false) => STYLES.phase() + STYLES.phase_pending(),
	};
	page!({
		li {
			class: classes,
			span {
				class: STYLES.phase_name(),
				{ name }
			}
			span {
				class: STYLES.phase_time(),
				{ time }
			}
		}
	})
}

fn log_line(line: LogLine) -> Page {
	let LogLine {
		time,
		warn,
		message,
	} = line;
	let level_classes = if warn {
		STYLES.log_level() + STYLES.level_warn()
	} else {
		STYLES.log_level() + STYLES.level_info()
	};
	let level = if warn { "WARN" } else { "INFO" };
	page!({
		li {
			class: STYLES.log_line(),
			span {
				class: STYLES.log_time(),
				{ time }
			}
			span {
				class: level_classes,
				{ level }
			}
			span { { message } }
		}
	})
}

/// Renders the preview. Put it in the `aside` slot of `signed_out_layout`.
pub fn sign_in_preview() -> Page {
	let deploy_classes = STYLES.card() + STYLES.deploy() + "rc-elevation-2";
	let log_classes = STYLES.card() + STYLES.log() + "rc-elevation-2" + "rc-font-mono";
	let project_chip = STYLES.chip() + STYLES.chip_project() + "rc-elevation-1";
	let cluster_chip = STYLES.chip() + STYLES.chip_cluster() + "rc-elevation-1";
	let title_classes = STYLES.card_title() + "rc-font-display";
	let lines: Vec<Page> = log_lines().into_iter().map(log_line).collect();
	let phases = vec![
		phase(
			true,
			false,
			IntoPage::into_page(t!("Submitted")),
			Page::text("14:28:02"),
		),
		phase(
			true,
			false,
			IntoPage::into_page(t!("Sent to Agent")),
			Page::text("14:28:03"),
		),
		phase(
			false,
			true,
			IntoPage::into_page(t!("Applying")),
			IntoPage::into_page(t!(
				"{ready} of {total} replicas ready",
				ready = "2",
				total = "3"
			)),
		),
		phase(
			false,
			false,
			IntoPage::into_page(t!("Running")),
			IntoPage::into_page(t!("Waiting")),
		),
	];
	let deployment_title = t!("Deployment {number}", number = DEPLOYMENT_NUMBER);
	let deployment_target = t!("Deployment {number}", number = DEPLOYMENT_NUMBER);
	let submitted = t!(
		"{project}, submitted {minutes} minutes ago by {user}",
		project = PROJECT,
		minutes = "4",
		user = SUBMITTER
	);
	let applying = badge(BadgeStatus::Progress, t!("Applying"));
	let running = badge(BadgeStatus::Success, t!("Running"));
	let connected = badge(BadgeStatus::Success, t!("Connected"));

	page!({
		div {
			class: "rc-still",
			div {
				class: project_chip,
				span { { PROJECT } }
				{ running }
			}
			div {
				class: deploy_classes,
				div {
					class: STYLES.head(),
					div {
						p {
							class: title_classes,
							{ deployment_title }
						}
						p {
							class: STYLES.card_sub(),
							{ submitted }
						}
					}
					{ applying }
				}
				ol {
					class: STYLES.phases(),
					for item in phases { { item } }
				}
				div {
					class: STYLES.facts(),
					span {
						{ t!("Cluster") }" " strong {
							class: STYLES.fact_value(),
							{ CLUSTER }
						}
					}
					span {
						{ t!("Commit") }" " strong {
							class: STYLES.fact_value(),
							{ COMMIT }
						}
					}
					span { { t!("Fix cart rounding for JPY") } }
				}
			}
			div {
				class: log_classes,
				div {
					class: STYLES.log_bar(),
					span {
						class: STYLES.log_live(),
						{ t!("Live") }
					}
					span {
						class: STYLES.log_target(),
						{ deployment_target }
					}
				}
				ol {
					class: STYLES.log_body(),
					for item in lines { { item } }
				}
			}
			div {
				class: cluster_chip,
				span { { CLUSTER } }
				{ connected }
			}
		}
	})
}
