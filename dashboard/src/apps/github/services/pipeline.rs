//! GitHub repository checkout and introspection pipeline.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use reinhardt_cloud_types::introspect::IntrospectOutput;
use tokio::process::Command;
use uuid::Uuid;

const INTROSPECT_TIMEOUT_ENV: &str = "REINHARDT_CLOUD_GITHUB_INTROSPECT_TIMEOUT_SECONDS";
const DEFAULT_INTROSPECT_TIMEOUT_SECONDS: u64 = 60;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct GitHubDeployPipelineInput {
	pub installation_id: i64,
	pub full_name: String,
	pub branch: String,
	pub app_name: String,
	pub namespace: String,
	pub registry: String,
	pub private: bool,
}

#[derive(Debug, Clone)]
pub struct GitHubDeployPipelineOutput {
	pub introspect: IntrospectOutput,
	pub credentials_secret: Option<String>,
}

#[derive(Debug)]
pub struct GitHubCheckout {
	root_dir: PathBuf,
	repository_dir: PathBuf,
}

impl GitHubCheckout {
	pub fn repository_dir(&self) -> &Path {
		&self.repository_dir
	}
}

impl Drop for GitHubCheckout {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.root_dir);
	}
}

pub fn github_credentials_secret_name(app_name: &str) -> String {
	format!("{app_name}-github-git-credentials")
}

pub fn credentials_secret_for_repository(app_name: &str, private: bool) -> Option<String> {
	private.then(|| github_credentials_secret_name(app_name))
}

pub async fn run_github_deploy_pipeline(
	input: &GitHubDeployPipelineInput,
	installation_token: &str,
) -> Result<GitHubDeployPipelineOutput, String> {
	let checkout =
		clone_repository_for_introspection(&input.full_name, &input.branch, installation_token)
			.await?;
	let introspect = run_manage_introspect(checkout.repository_dir()).await?;
	Ok(GitHubDeployPipelineOutput {
		introspect,
		credentials_secret: credentials_secret_for_repository(&input.app_name, input.private),
	})
}

pub async fn clone_repository_for_introspection(
	full_name: &str,
	branch: &str,
	installation_token: &str,
) -> Result<GitHubCheckout, String> {
	let root_dir = std::env::temp_dir().join(format!(
		"reinhardt-cloud-github-{}-{}",
		std::process::id(),
		Uuid::now_v7()
	));
	let repository_dir = root_dir.join("repository");
	std::fs::create_dir_all(&root_dir)
		.map_err(|e| format!("Failed to create checkout directory: {e}"))?;

	let checkout = GitHubCheckout {
		root_dir,
		repository_dir,
	};
	let clone_url = installation_clone_url(full_name, installation_token);
	let output = command_output_with_timeout(
		Command::new("git")
			.arg("clone")
			.arg("--depth")
			.arg("1")
			.arg("--branch")
			.arg(branch)
			.arg(&clone_url)
			.arg(checkout.repository_dir()),
		"git clone",
		Some(Duration::from_secs(120)),
	)
	.await;
	match output {
		Ok(_) => Ok(checkout),
		Err(err) => Err(err.replace(&clone_url, &redacted_clone_url(full_name))),
	}
}

pub async fn run_manage_introspect(project_dir: &Path) -> Result<IntrospectOutput, String> {
	let yaml = run_manage_introspect_yaml(project_dir).await?;
	serde_yaml::from_str(&yaml).map_err(|e| format!("Failed to parse introspect YAML: {e}"))
}

async fn run_manage_introspect_yaml(project_dir: &Path) -> Result<String, String> {
	let timeout = introspect_timeout();
	let project_manage = project_dir.join("manage");
	if project_manage.is_file() {
		return command_stdout(
			Command::new(project_manage)
				.arg("introspect")
				.arg("--format")
				.arg("yaml")
				.current_dir(project_dir),
			"manage introspect",
			timeout,
		)
		.await;
	}

	if let Ok(stdout) = command_stdout(
		Command::new("manage")
			.arg("introspect")
			.arg("--format")
			.arg("yaml")
			.current_dir(project_dir),
		"manage introspect",
		timeout,
	)
	.await
	{
		return Ok(stdout);
	}

	command_stdout(
		Command::new("cargo")
			.arg("run")
			.arg("--bin")
			.arg("manage")
			.arg("--")
			.arg("introspect")
			.arg("--format")
			.arg("yaml")
			.current_dir(project_dir),
		"cargo manage introspect",
		timeout,
	)
	.await
}

async fn command_stdout(
	command: &mut Command,
	label: &str,
	timeout: Option<Duration>,
) -> Result<String, String> {
	let output = command_output_with_timeout(command, label, timeout).await?;
	String::from_utf8(output.stdout).map_err(|e| format!("Invalid UTF-8 in {label} output: {e}"))
}

async fn command_output_with_timeout(
	command: &mut Command,
	label: &str,
	timeout: Option<Duration>,
) -> Result<std::process::Output, String> {
	command.stdout(Stdio::piped()).stderr(Stdio::piped());
	command.kill_on_drop(true);
	let child = command
		.spawn()
		.map_err(|e| format!("Failed to run {label}: {e}"))?;
	let output = match timeout {
		Some(timeout) => match tokio::time::timeout(timeout, child.wait_with_output()).await {
			Ok(output) => output.map_err(|e| format!("Failed to wait for {label}: {e}"))?,
			Err(_) => {
				return Err(format!(
					"{label} timed out after {} seconds",
					timeout.as_secs()
				));
			}
		},
		None => child
			.wait_with_output()
			.await
			.map_err(|e| format!("Failed to wait for {label}: {e}"))?,
	};
	if output.status.success() {
		Ok(output)
	} else {
		let stderr = String::from_utf8_lossy(&output.stderr);
		Err(format!("{label} failed: {stderr}"))
	}
}

pub fn parse_introspect_timeout_seconds(raw: &str) -> Option<Duration> {
	let seconds = raw.parse::<u64>().ok()?;
	if seconds == 0 {
		None
	} else {
		Some(Duration::from_secs(seconds))
	}
}

pub fn introspect_timeout() -> Option<Duration> {
	std::env::var(INTROSPECT_TIMEOUT_ENV)
		.ok()
		.and_then(|raw| parse_introspect_timeout_seconds(&raw))
		.or(Some(Duration::from_secs(
			DEFAULT_INTROSPECT_TIMEOUT_SECONDS,
		)))
}

pub fn installation_clone_url(full_name: &str, installation_token: &str) -> String {
	let encoded = utf8_percent_encode(installation_token, NON_ALPHANUMERIC).to_string();
	format!("https://x-access-token:{encoded}@github.com/{full_name}.git")
}

pub fn redacted_clone_url(full_name: &str) -> String {
	format!("https://x-access-token:[redacted]@github.com/{full_name}.git")
}

pub fn command_program_name(program: &OsStr) -> String {
	Path::new(program)
		.file_name()
		.and_then(OsStr::to_str)
		.unwrap_or_default()
		.to_string()
}
