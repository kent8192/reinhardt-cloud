//! Terraform HCL generation from ReinhardtApp CRD spec.

use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};
use reinhardt_cloud_core::infrastructure_derivation::{
	InfrastructureDerivationInput, derive_infrastructure_spec,
};
use reinhardt_cloud_types::crd::{
	ReinhardtAppSpec,
	infrastructure::{BucketSpec, DnsRecordSpec, InfrastructureSpec, PostgresSpec, SecretSpec},
};

/// Supported cloud providers for HCL generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Provider {
	Gcp,
	Aws,
}

impl Provider {
	fn as_str(self) -> &'static str {
		match self {
			Provider::Gcp => "gcp",
			Provider::Aws => "aws",
		}
	}
}

/// Terraform subcommand arguments.
#[derive(Debug, Args)]
pub(crate) struct TerraformArgs {
	#[command(subcommand)]
	pub(crate) command: TerraformCommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum TerraformCommand {
	/// Generate per-app Terraform HCL from a ReinhardtApp infrastructure spec.
	Generate(GenerateArgs),
}

/// Arguments for `terraform generate`.
#[derive(Debug, Args)]
pub(crate) struct GenerateArgs {
	/// Application name (used as a Terraform resource name prefix).
	#[arg(long)]
	pub(crate) app: String,

	/// Target cloud provider.
	#[arg(long, value_enum)]
	pub(crate) provider: Provider,

	/// Output directory; defaults to `./<app>-terraform`.
	#[arg(long)]
	pub(crate) output: Option<PathBuf>,

	/// Optional inline infrastructure JSON (for testing without a live cluster).
	/// If omitted, a minimal empty InfrastructureSpec is used.
	#[arg(long, hide = true)]
	pub(crate) infra_json: Option<String>,

	/// ReinhardtApp YAML manifest. Preferred source for spec.infrastructure.
	#[arg(long)]
	pub(crate) app_crd: Option<PathBuf>,
}

/// Execute the `terraform` subcommand.
pub(crate) async fn execute(args: &TerraformArgs) -> Result<(), Box<dyn std::error::Error>> {
	match &args.command {
		TerraformCommand::Generate(gen_args) => generate(gen_args).await,
	}
}

async fn generate(args: &GenerateArgs) -> Result<(), Box<dyn std::error::Error>> {
	let infra: InfrastructureSpec = match &args.infra_json {
		Some(json) => serde_json::from_str(json)?,
		None if args.app_crd.is_some() => {
			let app_crd = args.app_crd.as_ref().expect("app_crd is checked above");
			let yaml = std::fs::read_to_string(app_crd)?;
			infrastructure_from_crd_yaml(&args.app, &yaml)
				.map_err(|message| std::io::Error::new(std::io::ErrorKind::InvalidData, message))?
				.unwrap_or_default()
		}
		None => InfrastructureSpec::default(),
	};

	let out_dir = args
		.output
		.clone()
		.unwrap_or_else(|| PathBuf::from(format!("./{}-terraform", args.app)));

	std::fs::create_dir_all(&out_dir)?;

	let hcl = render_hcl(&args.app, args.provider, &infra);
	let out_path = out_dir.join("main.tf");
	std::fs::write(&out_path, &hcl)?;

	let versions_hcl = render_versions(args.provider);
	std::fs::write(out_dir.join("versions.tf"), &versions_hcl)?;

	eprintln!(
		"Generated Terraform for app '{}' (provider: {}) in {}",
		args.app,
		args.provider.as_str(),
		out_dir.display()
	);

	Ok(())
}

fn infrastructure_from_crd_yaml(
	app: &str,
	yaml: &str,
) -> Result<Option<InfrastructureSpec>, String> {
	let value: serde_yaml::Value = serde_yaml::from_str(yaml).map_err(|err| err.to_string())?;
	let spec_key = serde_yaml::Value::String("spec".to_string());
	let spec_value = value
		.as_mapping()
		.and_then(|mapping| mapping.get(&spec_key))
		.ok_or_else(|| "ReinhardtApp YAML is missing spec".to_string())?;
	let spec: ReinhardtAppSpec =
		serde_yaml::from_value(spec_value.clone()).map_err(|err| err.to_string())?;

	if let Some(infra) = spec.infrastructure {
		validate_infrastructure(&infra)?;
		return Ok(Some(infra));
	}

	if let Some(introspect) = spec.introspect.as_ref() {
		eprintln!(
			"warning: spec.infrastructure is absent; deriving from spec.introspect for compatibility. Persist the generated infrastructure block for repeatable Terraform."
		);
		let app_name = if introspect.app.name.trim().is_empty() {
			app.to_string()
		} else {
			introspect.app.name.clone()
		};

		return derive_infrastructure_spec(InfrastructureDerivationInput {
			app_name,
			signals: introspect.features.infrastructure_signals.clone(),
			explicit: None,
			typed_secret_refs: typed_secret_refs(&spec),
		})
		.map_err(|err| err.to_string());
	}

	Ok(None)
}

fn validate_infrastructure(infra: &InfrastructureSpec) -> Result<(), String> {
	infra.validate().map_err(|errors| {
		errors
			.into_iter()
			.map(|error| error.to_string())
			.collect::<Vec<_>>()
			.join("; ")
	})
}

fn typed_secret_refs(spec: &ReinhardtAppSpec) -> Vec<String> {
	[
		spec.source
			.as_ref()
			.and_then(|source| source.credentials_secret.as_ref()),
		spec.source
			.as_ref()
			.and_then(|source| source.webhook.as_ref())
			.and_then(|webhook| webhook.secret_ref.as_ref()),
		spec.mail
			.as_ref()
			.and_then(|mail| mail.credentials_secret.as_ref()),
	]
	.into_iter()
	.flatten()
	.cloned()
	.collect()
}

/// Renders the per-app `main.tf` content for the given provider.
pub(crate) fn render_hcl(app: &str, provider: Provider, infra: &InfrastructureSpec) -> String {
	match provider {
		Provider::Gcp => render_gcp_hcl(app, infra),
		Provider::Aws => render_aws_hcl(app, infra),
	}
}

fn render_versions(provider: Provider) -> String {
	match provider {
		Provider::Gcp => r#"terraform {
  required_version = ">= 1.7"

  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "~> 7.30"
    }
  }
}
"#
		.to_string(),
		Provider::Aws => r#"terraform {
  required_version = ">= 1.7"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.42"
    }
  }
}
"#
		.to_string(),
	}
}

fn render_gcp_hcl(app: &str, infra: &InfrastructureSpec) -> String {
	let mut blocks = Vec::new();

	blocks.push(format!(
		r#"# Generated by reinhardt-cloud terraform generate
# App: {app}
# Provider: gcp
#
# Variables expected from cluster bootstrap outputs (terraform/examples/gcp-minimal):
#   var.project_id, var.region, var.network_id, var.name_prefix

variable "project_id" {{
  description = "GCP project ID."
  type        = string
}}

variable "region" {{
  description = "GCP region."
  type        = string
  default     = "us-central1"
}}

variable "network_id" {{
  description = "Self-link of the VPC network (from bootstrap outputs)."
  type        = string
}}

variable "name_prefix" {{
  description = "Bootstrap name prefix (e.g., reinhardt-prod)."
  type        = string
}}
"#
	));

	if let Some(ref pg) = infra.postgres {
		blocks.push(render_gcp_cloudsql(app, pg));
	}

	if let Some(ref buckets) = infra.buckets {
		for bucket in buckets {
			blocks.push(render_gcp_bucket(app, bucket));
		}
	}

	if let Some(ref dns_records) = infra.dns {
		for record in dns_records {
			blocks.push(render_gcp_dns(app, record));
		}
	}

	if let Some(ref secrets) = infra.secrets {
		for secret in secrets {
			blocks.push(render_gcp_secret(app, secret));
		}
	}

	blocks.join("\n")
}

fn render_gcp_cloudsql(app: &str, pg: &PostgresSpec) -> String {
	let tier = pg.tier.as_deref().unwrap_or("db-f1-micro");
	let version = pg
		.version
		.as_deref()
		.map(|v| format!("POSTGRES_{}", v.replace('.', "_")))
		.unwrap_or_else(|| "POSTGRES_16".to_string());
	let retention = pg.backup_retention_days.unwrap_or(7);

	format!(
		r#"module "{app}_postgres" {{
  source      = "../../modules/gcp/per-app/cloudsql"
  name_prefix = "${{var.name_prefix}}-{app}"
  project_id  = var.project_id
  region      = var.region
  network_id  = var.network_id
  tier        = "{tier}"
  database_version      = "{version}"
  backup_retention_days = {retention}
}}
"#
	)
}

fn render_gcp_bucket(app: &str, bucket: &BucketSpec) -> String {
	let public = bucket.public;
	let name = &bucket.name;

	format!(
		r#"module "{app}_bucket_{name}" {{
  source      = "../../modules/gcp/per-app/gcs"
  name_prefix = "${{var.name_prefix}}-{app}"
  project_id  = var.project_id
  bucket_name = "{name}"
  public      = {public}
}}
"#
	)
}

fn render_gcp_dns(app: &str, record: &DnsRecordSpec) -> String {
	let host = &record.host;
	let record_type = &record.record_type;
	let safe_host = host.replace('.', "_");

	format!(
		r#"module "{app}_dns_{safe_host}" {{
  source      = "../../modules/gcp/per-app/dns"
  name_prefix = "${{var.name_prefix}}-{app}"
  project_id  = var.project_id
  host        = "{host}"
  record_type = "{record_type}"
}}
"#
	)
}

fn render_gcp_secret(app: &str, secret: &SecretSpec) -> String {
	let name = &secret.name;
	let description = secret
		.description
		.as_deref()
		.unwrap_or("Application secret");

	format!(
		r#"module "{app}_secret_{name}" {{
  source      = "../../modules/gcp/per-app/secrets"
  name_prefix = "${{var.name_prefix}}-{app}"
  project_id  = var.project_id
  secret_name = "{name}"
  description = "{description}"
}}
"#
	)
}

fn render_aws_hcl(app: &str, infra: &InfrastructureSpec) -> String {
	let mut blocks = Vec::new();

	blocks.push(format!(
		r#"# Generated by reinhardt-cloud terraform generate
# App: {app}
# Provider: aws
#
# Variables expected from cluster bootstrap outputs (terraform/examples/aws-minimal):
#   var.vpc_id, var.subnet_ids, var.name_prefix

variable "vpc_id" {{
  description = "VPC ID (from bootstrap outputs)."
  type        = string
}}

variable "subnet_ids" {{
  description = "Private subnet IDs (from bootstrap outputs)."
  type        = list(string)
}}

variable "name_prefix" {{
  description = "Bootstrap name prefix (e.g., reinhardt-prod)."
  type        = string
}}
"#
	));

	if let Some(ref pg) = infra.postgres {
		blocks.push(render_aws_rds(app, pg));
	}

	if let Some(ref buckets) = infra.buckets {
		for bucket in buckets {
			blocks.push(render_aws_bucket(app, bucket));
		}
	}

	if let Some(ref dns_records) = infra.dns {
		for record in dns_records {
			blocks.push(render_aws_dns(app, record));
		}
	}

	if let Some(ref secrets) = infra.secrets {
		for secret in secrets {
			blocks.push(render_aws_secret(app, secret));
		}
	}

	blocks.join("\n")
}

fn render_aws_rds(app: &str, pg: &PostgresSpec) -> String {
	let tier = pg.tier.as_deref().unwrap_or("db.t3.micro");
	let version = pg.version.as_deref().unwrap_or("16.3");
	let retention = pg.backup_retention_days.unwrap_or(7);

	format!(
		r#"module "{app}_rds" {{
  source      = "../../modules/aws/per-app/rds"
  name_prefix = "${{var.name_prefix}}-{app}"
  vpc_id      = var.vpc_id
  subnet_ids  = var.subnet_ids
  instance_class        = "{tier}"
  engine_version        = "{version}"
  backup_retention_days = {retention}
  db_password           = var.{app}_db_password
}}

variable "{app}_db_password" {{
  description = "Master password for the {app} RDS instance."
  type        = string
  sensitive   = true
}}
"#
	)
}

fn render_aws_bucket(app: &str, bucket: &BucketSpec) -> String {
	let name = &bucket.name;
	let public = bucket.public;

	format!(
		r#"module "{app}_bucket_{name}" {{
  source      = "../../modules/aws/per-app/s3"
  name_prefix = "${{var.name_prefix}}-{app}"
  bucket_name = "{name}"
  public      = {public}
}}
"#
	)
}

fn render_aws_dns(app: &str, record: &DnsRecordSpec) -> String {
	let host = &record.host;
	let record_type = &record.record_type;
	let safe_host = host.replace('.', "_");

	format!(
		r#"module "{app}_dns_{safe_host}" {{
  source      = "../../modules/aws/per-app/route53"
  name_prefix = "${{var.name_prefix}}-{app}"
  host        = "{host}"
  record_type = "{record_type}"
}}
"#
	)
}

fn render_aws_secret(app: &str, secret: &SecretSpec) -> String {
	let name = &secret.name;
	let description = secret
		.description
		.as_deref()
		.unwrap_or("Application secret");

	format!(
		r#"module "{app}_secret_{name}" {{
  source      = "../../modules/aws/per-app/secretsmanager"
  name_prefix = "${{var.name_prefix}}-{app}"
  secret_name = "{name}"
  description = "{description}"
}}
"#
	)
}

#[cfg(test)]
mod tests {
	use reinhardt_cloud_types::crd::infrastructure::{
		BucketSpec, DnsRecordSpec, InfrastructureSpec, PostgresSpec, SecretSpec,
	};

	use super::*;
	use rstest::rstest;

	#[rstest]
	fn render_gcp_empty_infra_contains_variables() {
		// Arrange
		let infra = InfrastructureSpec::default();

		// Act
		let hcl = render_hcl("myapp", Provider::Gcp, &infra);

		// Assert
		assert!(hcl.contains("variable \"project_id\""));
		assert!(hcl.contains("variable \"region\""));
		assert!(hcl.contains("variable \"network_id\""));
	}

	#[rstest]
	fn render_aws_empty_infra_contains_variables() {
		// Arrange
		let infra = InfrastructureSpec::default();

		// Act
		let hcl = render_hcl("myapp", Provider::Aws, &infra);

		// Assert
		assert!(hcl.contains("variable \"vpc_id\""));
		assert!(hcl.contains("variable \"subnet_ids\""));
		assert!(hcl.contains("variable \"name_prefix\""));
	}

	#[rstest]
	fn render_gcp_postgres_block() {
		// Arrange
		let infra = InfrastructureSpec {
			postgres: Some(PostgresSpec {
				tier: Some("db-custom-2-4096".to_string()),
				version: Some("16".to_string()),
				backup_retention_days: Some(7),
			}),
			..Default::default()
		};

		// Act
		let hcl = render_hcl("orders", Provider::Gcp, &infra);

		// Assert
		assert!(hcl.contains("module \"orders_postgres\""));
		assert!(hcl.contains("db-custom-2-4096"));
		assert!(hcl.contains("POSTGRES_16"));
		assert!(hcl.contains("backup_retention_days = 7"));
	}

	#[rstest]
	fn render_aws_rds_block() {
		// Arrange
		let infra = InfrastructureSpec {
			postgres: Some(PostgresSpec {
				tier: Some("db.t3.micro".to_string()),
				version: Some("16.3".to_string()),
				backup_retention_days: Some(14),
			}),
			..Default::default()
		};

		// Act
		let hcl = render_hcl("orders", Provider::Aws, &infra);

		// Assert
		assert!(hcl.contains("module \"orders_rds\""));
		assert!(hcl.contains("db.t3.micro"));
		assert!(hcl.contains("backup_retention_days = 14"));
		assert!(hcl.contains("variable \"orders_db_password\""));
	}

	#[rstest]
	fn render_gcp_bucket_public_false() {
		// Arrange
		let infra = InfrastructureSpec {
			buckets: Some(vec![BucketSpec {
				name: "assets".to_string(),
				public: false,
			}]),
			..Default::default()
		};

		// Act
		let hcl = render_hcl("myapp", Provider::Gcp, &infra);

		// Assert
		assert!(hcl.contains("module \"myapp_bucket_assets\""));
		assert!(hcl.contains("false"));
	}

	#[rstest]
	fn render_gcp_dns_record() {
		// Arrange
		let infra = InfrastructureSpec {
			dns: Some(vec![DnsRecordSpec {
				host: "orders.example.com".to_string(),
				record_type: "A".to_string(),
			}]),
			..Default::default()
		};

		// Act
		let hcl = render_hcl("orders", Provider::Gcp, &infra);

		// Assert
		assert!(hcl.contains("module \"orders_dns_orders_example_com\""));
		assert!(hcl.contains("orders.example.com"));
		assert!(hcl.contains("\"A\""));
	}

	#[rstest]
	fn render_gcp_secret() {
		// Arrange
		let infra = InfrastructureSpec {
			secrets: Some(vec![SecretSpec {
				name: "api-key".to_string(),
				description: Some("Third-party API key".to_string()),
			}]),
			..Default::default()
		};

		// Act
		let hcl = render_hcl("myapp", Provider::Gcp, &infra);

		// Assert
		assert!(hcl.contains("module \"myapp_secret_api-key\""));
		assert!(hcl.contains("description = \"Third-party API key\""));
	}

	#[rstest]
	fn render_gcp_full_infra_golden() {
		// Arrange
		let infra = InfrastructureSpec {
			postgres: Some(PostgresSpec {
				tier: Some("db-f1-micro".to_string()),
				version: Some("16".to_string()),
				backup_retention_days: Some(7),
			}),
			buckets: Some(vec![BucketSpec {
				name: "uploads".to_string(),
				public: false,
			}]),
			dns: Some(vec![DnsRecordSpec {
				host: "api.example.com".to_string(),
				record_type: "A".to_string(),
			}]),
			secrets: Some(vec![SecretSpec {
				name: "stripe-key".to_string(),
				description: None,
			}]),
		};

		// Act
		let hcl = render_hcl("sample", Provider::Gcp, &infra);

		// Assert — golden checks for each resource block
		assert!(hcl.contains("module \"sample_postgres\""));
		assert!(hcl.contains("module \"sample_bucket_uploads\""));
		assert!(hcl.contains("module \"sample_dns_api_example_com\""));
		assert!(hcl.contains("module \"sample_secret_stripe-key\""));
	}

	#[rstest]
	fn render_aws_full_infra_golden() {
		// Arrange
		let infra = InfrastructureSpec {
			postgres: Some(PostgresSpec {
				tier: Some("db.t3.micro".to_string()),
				version: Some("16.3".to_string()),
				backup_retention_days: Some(7),
			}),
			buckets: Some(vec![BucketSpec {
				name: "uploads".to_string(),
				public: false,
			}]),
			dns: Some(vec![DnsRecordSpec {
				host: "api.example.com".to_string(),
				record_type: "CNAME".to_string(),
			}]),
			secrets: Some(vec![SecretSpec {
				name: "db-creds".to_string(),
				description: None,
			}]),
		};

		// Act
		let hcl = render_hcl("sample", Provider::Aws, &infra);

		// Assert
		assert!(hcl.contains("module \"sample_rds\""));
		assert!(hcl.contains("module \"sample_bucket_uploads\""));
		assert!(hcl.contains("module \"sample_dns_api_example_com\""));
		assert!(hcl.contains("module \"sample_secret_db-creds\""));
	}

	#[rstest]
	fn extracts_explicit_infrastructure_from_crd_yaml() {
		// Arrange
		let yaml = r#"
apiVersion: cloud.reinhardt.dev/v1alpha1
kind: ReinhardtApp
metadata:
  name: orders
spec:
  image: ghcr.io/example/orders:latest
  infrastructure:
    postgres:
      version: "16"
      backup_retention_days: 7
"#;

		// Act
		let infra = infrastructure_from_crd_yaml("orders", yaml)
			.expect("CRD YAML should parse")
			.expect("explicit infrastructure should be extracted");

		// Assert
		assert!(infra.postgres.is_some());
	}

	#[rstest]
	fn rejects_invalid_explicit_infrastructure_from_crd_yaml() {
		// Arrange
		let yaml = r#"
apiVersion: cloud.reinhardt.dev/v1alpha1
kind: ReinhardtApp
metadata:
  name: orders
spec:
  image: ghcr.io/example/orders:latest
  infrastructure:
    buckets:
      - name: ""
        public: false
"#;

		// Act
		let error = infrastructure_from_crd_yaml("orders", yaml)
			.expect_err("invalid explicit infrastructure should fail early");

		// Assert
		assert!(
			error.contains("infrastructure.buckets[].name must be non-empty"),
			"unexpected error: {error}"
		);
	}

	#[rstest]
	fn falls_back_to_introspect_when_infrastructure_missing() {
		// Arrange
		let yaml = r#"
apiVersion: cloud.reinhardt.dev/v1alpha1
kind: ReinhardtApp
metadata:
  name: orders
spec:
  image: ghcr.io/example/orders:latest
  introspect:
    app:
      name: orders
    features:
      infrastructure_signals:
        database: postgres
"#;

		// Act
		let infra = infrastructure_from_crd_yaml("fallback-app", yaml)
			.expect("introspect fallback should succeed")
			.expect("postgres signal should derive infrastructure");

		// Assert
		assert!(infra.postgres.is_some());
	}

	#[rstest]
	fn fallback_preserves_typed_secret_refs() {
		// Arrange
		let yaml = r#"
apiVersion: cloud.reinhardt.dev/v1alpha1
kind: ReinhardtApp
metadata:
  name: orders
spec:
  image: ghcr.io/example/orders:latest
  source:
    repository: https://github.com/example/orders
    credentials_secret: git-creds
    webhook:
      enabled: true
      secret_ref: webhook-secret
      events:
        - push
  mail:
    smtp_host: smtp.example.com
    smtp_port: 587
    credentials_secret: mail-creds
  introspect:
    app:
      name: orders
    features:
      infrastructure_signals:
        database: postgres
"#;

		// Act
		let infra = infrastructure_from_crd_yaml("orders", yaml)
			.expect("introspect fallback should succeed")
			.expect("postgres signal should derive infrastructure");

		// Assert
		let secret_names: Vec<_> = infra
			.secrets
			.expect("typed secret refs should be preserved")
			.into_iter()
			.map(|secret| secret.name)
			.collect();
		assert_eq!(
			secret_names,
			vec![
				"git-creds".to_string(),
				"mail-creds".to_string(),
				"webhook-secret".to_string()
			]
		);
	}

	#[rstest]
	fn fallback_fails_on_unsupported_storage() {
		// Arrange
		let yaml = r#"
apiVersion: cloud.reinhardt.dev/v1alpha1
kind: ReinhardtApp
metadata:
  name: orders
spec:
  image: ghcr.io/example/orders:latest
  introspect:
    app:
      name: orders
    features:
      infrastructure_signals:
        storage: local
"#;

		// Act
		let error =
			infrastructure_from_crd_yaml("orders", yaml).expect_err("local storage should fail");

		// Assert
		assert!(
			error.contains("unsupported managed storage backend `local`"),
			"unexpected error: {error}"
		);
	}
}
