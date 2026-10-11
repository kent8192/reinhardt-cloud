//! Process-wide log output.
//!
//! Nothing in the framework installs a `tracing` subscriber, so without one the
//! `audit` events (see [`crate::audit`]) and every error log of the server and
//! of the `manage` commands would be dropped. [`init`] installs one: JSON lines
//! on standard error, filtered by `RUST_LOG` (default `info`, which includes the
//! audit events). Standard output stays free for command results, such as the
//! Login Link URL, that a script captures.

use tracing_subscriber::EnvFilter;

/// Install the process-wide subscriber, unless one is already installed.
pub fn init() {
	let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
	// An already-installed subscriber (a test harness, an embedding process) is
	// kept: `try_init` fails then, and there is nothing to report.
	let _ = tracing_subscriber::fmt()
		.json()
		.with_env_filter(filter)
		.with_writer(std::io::stderr)
		.try_init();
}
