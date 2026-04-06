//! Credentials management commands for GitHub/GitLab integration.

use clap::{Args, Subcommand};
use std::collections::BTreeMap;
use std::path::PathBuf;

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
	pub git_token: Option<String>,
	#[arg(long)]
	pub registry_auth: Option<PathBuf>,
	#[arg(long)]
	pub webhook_secret: Option<String>,
	#[arg(long)]
	pub api_token: Option<String>,
	#[arg(long)]
	pub secret_name: Option<String>,
	#[arg(long, default_value = "default")]
	pub namespace: String,
}

#[derive(Debug, Args)]
pub(crate) struct CheckArgs {
	pub app_name: String,
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

	if let Some(ref token) = args.git_token {
		string_data.insert("git-token".to_string(), token.clone());
	}
	if let Some(ref path) = args.registry_auth {
		let content = std::fs::read_to_string(path)
			.map_err(|e| format!("Failed to read registry-auth file {}: {e}", path.display()))?;
		string_data.insert("registry-auth".to_string(), content);
	}
	if let Some(ref secret) = args.webhook_secret {
		string_data.insert("webhook-secret".to_string(), secret.clone());
	}
	if let Some(ref token) = args.api_token {
		string_data.insert("api-token".to_string(), token.clone());
	}

	if string_data.is_empty() {
		return Err("At least one credential flag must be provided".into());
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

	println!(
		"Secret '{secret_name}' applied in namespace '{}'",
		args.namespace
	);
	Ok(())
}

/// Checks credential status for an app by looking up {app_name}-git-credentials.
async fn execute_check(args: &CheckArgs) -> Result<(), Box<dyn std::error::Error>> {
	let client = Client::try_default().await?;
	let secrets: Api<Secret> = Api::namespaced(client, &args.namespace);

	let secret_name = format!("{}-git-credentials", args.app_name);

	match secrets.get_opt(&secret_name).await? {
		Some(secret) => {
			println!(
				"Secret '{secret_name}' found in namespace '{}'",
				args.namespace
			);

			if let Some(labels) = &secret.metadata.labels
				&& let Some(provider) = labels.get("reinhardt.dev/provider")
			{
				println!("  Provider: {provider}");
			}

			let mut keys = Vec::new();
			if let Some(ref data) = secret.data {
				keys.extend(data.keys().cloned());
			}
			if let Some(ref string_data) = secret.string_data {
				keys.extend(string_data.keys().cloned());
			}

			if keys.is_empty() {
				println!("  Keys: (none)");
			} else {
				keys.sort();
				println!("  Keys: {}", keys.join(", "));
			}
		}
		None => {
			println!(
				"Secret '{secret_name}' not found in namespace '{}'",
				args.namespace
			);
		}
	}

	Ok(())
}
