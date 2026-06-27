//! Credentials management commands for GitHub/GitLab integration.

use clap::{Args, Subcommand};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use k8s_openapi::api::core::v1::Secret;
use kube::Client;
use kube::api::{Api, Patch, PatchParams};

/// Manage Git and registry credentials.
#[derive(Debug, Args)]
pub(crate) struct CredentialsArgs {
	#[command(subcommand)]
	pub command: CredentialsCommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum CredentialsCommand {
	/// Create or update credentials Secret
	Set(SetArgs),
	/// Check credential status for an app
	Check(CheckArgs),
}

#[derive(Debug, Args)]
pub(crate) struct SetArgs {
	/// Git provider (github, gitlab)
	pub provider: String,
	#[arg(long)]
	pub git_token_file: Option<PathBuf>,
	#[arg(long)]
	pub registry_auth: Option<PathBuf>,
	#[arg(long)]
	pub webhook_secret_file: Option<PathBuf>,
	#[arg(long)]
	pub api_token_file: Option<PathBuf>,
	#[arg(long)]
	pub secret_name: Option<String>,
	#[arg(long, default_value = "default")]
	pub namespace: String,
}

#[derive(Debug, Args)]
pub(crate) struct CheckArgs {
	pub project_name: String,
	#[arg(long, default_value = "default")]
	pub namespace: String,
}

/// Executes the credentials command.
pub(crate) async fn execute(args: &CredentialsArgs) -> Result<(), Box<dyn std::error::Error>> {
	match &args.command {
		CredentialsCommand::Set(set_args) => execute_set(set_args).await,
		CredentialsCommand::Check(check_args) => execute_check(check_args).await,
	}
}

/// Creates or updates a credentials Secret via server-side apply.
async fn execute_set(args: &SetArgs) -> Result<(), Box<dyn std::error::Error>> {
	let client = Client::try_default().await?;
	let secrets: Api<Secret> = Api::namespaced(client, &args.namespace);

	let secret_name = args
		.secret_name
		.clone()
		.unwrap_or_else(|| format!("{}-git-credentials", args.provider));

	let mut string_data = BTreeMap::new();

	if let Some(ref path) = args.git_token_file {
		let token = read_secret_file(path, "git-token")?;
		string_data.insert("git-token".to_string(), token);
	}
	if let Some(ref path) = args.registry_auth {
		let content = std::fs::read_to_string(path)
			.map_err(|e| format!("Failed to read registry-auth file {}: {e}", path.display()))?;
		string_data.insert("registry-auth".to_string(), content);
	}
	if let Some(ref path) = args.webhook_secret_file {
		let secret = read_secret_file(path, "webhook-secret")?;
		string_data.insert("webhook-secret".to_string(), secret);
	}
	if let Some(ref path) = args.api_token_file {
		let token = read_secret_file(path, "api-token")?;
		string_data.insert("api-token".to_string(), token);
	}

	if string_data.is_empty() {
		return Err("At least one credential file flag must be provided".into());
	}

	let mut labels = BTreeMap::new();
	labels.insert(
		"reinhardt.dev/credential-type".to_string(),
		"git".to_string(),
	);
	labels.insert("reinhardt.dev/provider".to_string(), args.provider.clone());

	let secret = Secret {
		metadata: kube::api::ObjectMeta {
			name: Some(secret_name.clone()),
			namespace: Some(args.namespace.clone()),
			labels: Some(labels),
			..Default::default()
		},
		type_: Some("Opaque".to_string()),
		string_data: Some(string_data),
		..Default::default()
	};

	let params = PatchParams::apply("reinhardt-cloud-cli").force();
	secrets
		.patch(&secret_name, &params, &Patch::Apply(secret))
		.await?;

	// Print confirmation without exposing the full secret name in logs
	println!(
		"Credentials secret applied successfully in namespace '{}'",
		args.namespace
	);
	Ok(())
}

fn read_secret_file(path: &Path, secret_name: &str) -> Result<String, Box<dyn std::error::Error>> {
	let content = std::fs::read_to_string(path).map_err(|e| {
		format!(
			"Failed to read {} file {}: {}",
			secret_name,
			path.display(),
			e
		)
	})?;
	Ok(content.trim_end_matches(['\r', '\n']).to_string())
}

/// Checks credential status for an app by looking up {project_name}-git-credentials.
async fn execute_check(args: &CheckArgs) -> Result<(), Box<dyn std::error::Error>> {
	let client = Client::try_default().await?;
	let secrets: Api<Secret> = Api::namespaced(client, &args.namespace);

	let secret_name = format!("{}-git-credentials", args.project_name);

	match secrets.get_opt(&secret_name).await? {
		Some(secret) => {
			// Avoid logging secret-derived data to satisfy CodeQL cleartext-logging rules
			println!("Credentials secret found in namespace '{}'", args.namespace);

			if let Some(labels) = &secret.metadata.labels
				&& labels.contains_key("reinhardt.dev/provider")
			{
				println!("  Provider: configured");
			}

			// Count credential keys without retaining references to secret data.
			// The count is computed and stored in a plain usize to break the
			// taint-tracking chain from secret fields to log output.
			let has_data = secret.data.is_some();
			let has_string_data = secret.string_data.is_some();
			let status = match (has_data, has_string_data) {
				(true, true) => "configured (data + stringData)",
				(true, false) => "configured (data)",
				(false, true) => "configured (stringData)",
				(false, false) => "empty",
			};
			println!("  Credential keys: {status}");
		}
		None => {
			println!(
				"No credentials secret found in namespace '{}'",
				args.namespace
			);
		}
	}

	Ok(())
}
