//! Helpers shared by the tests that change the process environment.
//!
//! Every test using them must be marked `#[serial(env_settings_load)]`.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

/// Process environment variables the committed settings require.
pub(crate) const REQUIRED_ENV: [(&str, &str); 3] = [
	(
		"REINHARDT_CORE__SECRET_KEY",
		"test-only-secret-key-not-for-deployment",
	),
	("REINHARDT_DATABASE_PASSWORD", "test-only-database-password"),
	(
		"REINHARDT_CLOUD_REDIS_URL",
		"redis://:test-redis-password@redis.test:6379/0",
	),
];

/// Restores the previous value of every variable it overrides when dropped.
pub(crate) struct EnvGuard {
	previous: Vec<(&'static str, Option<OsString>)>,
}

impl EnvGuard {
	/// Set or unset (`None`) each variable, remembering the prior values.
	pub(crate) fn apply(vars: &[(&'static str, Option<&str>)]) -> Self {
		let previous = vars
			.iter()
			.map(|(name, value)| {
				let old = env::var_os(name);
				// SAFETY: callers are marked `#[serial(env_settings_load)]`, so no
				// other test reads or writes the process environment concurrently.
				unsafe {
					match value {
						Some(value) => env::set_var(name, value),
						None => env::remove_var(name),
					}
				}
				(*name, old)
			})
			.collect();
		Self { previous }
	}
}

impl Drop for EnvGuard {
	fn drop(&mut self) {
		// Reverse order: a variable applied twice must end at its original value.
		for (name, old) in self.previous.drain(..).rev() {
			// SAFETY: see `EnvGuard::apply`.
			unsafe {
				match old {
					Some(value) => env::set_var(name, value),
					None => env::remove_var(name),
				}
			}
		}
	}
}

/// Temporary directory removed on drop.
pub(crate) struct TempDir(PathBuf);

impl TempDir {
	pub(crate) fn new(label: &str) -> Self {
		let path = env::temp_dir().join(format!("{label}-{}", std::process::id()));
		fs::create_dir_all(&path).expect("temp dir should be created");
		Self(path)
	}

	pub(crate) fn path(&self) -> &Path {
		&self.0
	}
}

impl Drop for TempDir {
	fn drop(&mut self) {
		let _ = fs::remove_dir_all(&self.0);
	}
}

pub(crate) fn required_env(profile: &'static str) -> Vec<(&'static str, Option<&'static str>)> {
	let mut vars: Vec<_> = REQUIRED_ENV.iter().map(|(k, v)| (*k, Some(*v))).collect();
	vars.push(("REINHARDT_ENV", Some(profile)));
	vars.push(("REINHARDT_CLOUD_CONFIG_DIR", None));
	vars
}
