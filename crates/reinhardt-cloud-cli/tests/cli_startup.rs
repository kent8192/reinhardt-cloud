//! Integration tests verifying CLI startup config + token wiring (Refs #372).

use rstest::rstest;
use std::process::Command;

fn cli_binary() -> std::path::PathBuf {
	std::path::PathBuf::from(env!("CARGO_BIN_EXE_reinhardt-cloud"))
}

#[rstest]
fn test_cli_runs_with_missing_config_file() {
	// Arrange: point HOME and XDG_CONFIG_HOME at an empty temp dir so
	// ~/.config/reinhardt-cloud/config.toml does not exist.
	let home = tempfile::tempdir().unwrap();

	// Act
	let output = Command::new(cli_binary())
		.env("HOME", home.path())
		.env("XDG_CONFIG_HOME", home.path().join(".config"))
		.arg("--help")
		.output()
		.expect("CLI should execute");

	// Assert
	assert!(
		output.status.success(),
		"CLI exited non-zero with missing config: stderr={}",
		String::from_utf8_lossy(&output.stderr)
	);
}

#[rstest]
fn test_cli_loads_api_url_from_config_file() {
	// Arrange: create config.toml with a distinctive api_url the CLI must pick
	// up. `status --name nonexistent` will try to reach it and fail, but the
	// resolved URL surfaces either in the `Target:` banner (stdout) or in the
	// connect-refused error from reqwest (stderr).
	let home = tempfile::tempdir().unwrap();
	// Cover both platform conventions so the test is OS-independent:
	// - Linux / XDG: $XDG_CONFIG_HOME/reinhardt-cloud/
	// - macOS:       $HOME/Library/Application Support/reinhardt-cloud/
	// - Windows:     %APPDATA%/reinhardt-cloud/ (dirs uses RoamingAppData)
	let xdg_dir = home.path().join(".config").join("reinhardt-cloud");
	let macos_dir = home
		.path()
		.join("Library")
		.join("Application Support")
		.join("reinhardt-cloud");
	for dir in [&xdg_dir, &macos_dir] {
		std::fs::create_dir_all(dir).unwrap();
		std::fs::write(
			dir.join("config.toml"),
			r#"
api_url = "http://loaded-from-file.example.com:9000"
"#,
		)
		.unwrap();
	}

	// Act
	let output = Command::new(cli_binary())
		.env("HOME", home.path())
		.env("XDG_CONFIG_HOME", home.path().join(".config"))
		.args(["status", "--name", "nonexistent"])
		.output()
		.expect("CLI should execute");

	// Assert
	let combined = format!(
		"{}{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(
		combined.contains("loaded-from-file.example.com:9000"),
		"expected configured URL in CLI output; got: {combined}"
	);
}
