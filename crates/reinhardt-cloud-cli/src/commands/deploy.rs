//! Deploy command: deploys an application to the Reinhardt Cloud platform.

use clap::Args;
use std::path::PathBuf;
use std::process::Command;
use tokio::io::AsyncWriteExt;

use crate::client::ReinhardtCloudClient;
use reinhardt_cloud_types::introspect::IntrospectOutput;
use reinhardt_cloud_types::reinhardt_cloud_toml::ReinhardtCloudToml;

/// Deploy an application.
#[derive(Debug, Args)]
pub(crate) struct DeployArgs {
	/// Application name (overrides reinhardt-cloud.toml if set)
	#[arg(short, long)]
	pub name: Option<String>,

	/// Docker image to deploy (overrides reinhardt-cloud.toml if set)
	#[arg(short, long)]
	pub image: Option<String>,

	/// Number of replicas
	#[arg(short, long)]
	pub replicas: Option<u32>,

	/// Project directory (defaults to current directory)
	#[arg(short, long)]
	pub dir: Option<PathBuf>,

	/// Output CRD YAML without applying
	#[arg(long)]
	pub dry_run: bool,

	/// Skip control plane, apply CRD directly to K8s
	#[arg(long)]
	pub direct: bool,

	/// Run introspect and display results only
	#[arg(long)]
	pub introspect_only: bool,

	/// Kubernetes namespace
	#[arg(long, default_value = "default")]
	pub namespace: String,

	/// Target cluster name
	#[arg(long)]
	pub cluster: Option<String>,
}

/// Reads reinhardt-cloud.toml from the project directory if it exists.
///
/// Returns `Ok(None)` when the file does not exist, `Ok(Some(...))` on
/// successful parse, and `Err` when the file exists but cannot be read
/// or contains malformed TOML.
fn read_reinhardt_cloud_toml(dir: &std::path::Path) -> Result<Option<ReinhardtCloudToml>, String> {
	let path = dir.join("reinhardt-cloud.toml");
	if !path.exists() {
		return Ok(None);
	}
	let content = std::fs::read_to_string(&path)
		.map_err(|e| format!("Failed to read reinhardt-cloud.toml: {e}"))?;
	let config: ReinhardtCloudToml = toml::from_str(&content)
		.map_err(|e| format!("Failed to parse reinhardt-cloud.toml: {e}"))?;
	Ok(Some(config))
}

/// Parses YAML output from `manage introspect` into an `IntrospectOutput`.
fn parse_introspect_output(yaml: &str) -> Result<IntrospectOutput, String> {
	serde_yaml::from_str(yaml).map_err(|e| format!("Failed to parse introspect YAML: {e}"))
}

/// Runs `manage introspect --format yaml` and returns stdout.
///
/// Tries the production binary first, then falls back to
/// `cargo run -- introspect --format yaml` for development mode.
fn run_manage_introspect() -> Result<String, String> {
	// Try production binary first
	let result = Command::new("manage")
		.args(["introspect", "--format", "yaml"])
		.output();

	if let Ok(output) = result
		&& output.status.success()
	{
		return String::from_utf8(output.stdout)
			.map_err(|e| format!("Invalid UTF-8 in manage output: {e}"));
	}

	// Fall back to cargo run for development mode
	let result = Command::new("cargo")
		.args([
			"run",
			"--bin",
			"manage",
			"--",
			"introspect",
			"--format",
			"yaml",
		])
		.output()
		.map_err(|e| format!("Failed to run cargo: {e}"))?;

	if result.status.success() {
		String::from_utf8(result.stdout).map_err(|e| format!("Invalid UTF-8 in cargo output: {e}"))
	} else {
		let stderr = String::from_utf8_lossy(&result.stderr);
		Err(format!("manage introspect failed: {stderr}"))
	}
}

/// Builds a `ReinhardtApp` CRD YAML value with optional introspect data.
fn build_reinhardt_app_crd(
	name: &str,
	namespace: &str,
	image: &str,
	replicas: Option<i32>,
	introspect: Option<IntrospectOutput>,
) -> serde_yaml::Value {
	let mut spec = serde_yaml::Mapping::new();
	spec.insert(
		serde_yaml::Value::String("image".to_string()),
		serde_yaml::Value::String(image.to_string()),
	);
	if let Some(r) = replicas {
		spec.insert(
			serde_yaml::Value::String("replicas".to_string()),
			serde_yaml::Value::Number(serde_yaml::Number::from(r)),
		);
	}
	if let Some(intro) = introspect {
		let intro_value = serde_yaml::to_value(&intro).unwrap_or(serde_yaml::Value::Null);
		spec.insert(
			serde_yaml::Value::String("introspect".to_string()),
			intro_value,
		);
	}

	let mut metadata = serde_yaml::Mapping::new();
	metadata.insert(
		serde_yaml::Value::String("name".to_string()),
		serde_yaml::Value::String(name.to_string()),
	);
	metadata.insert(
		serde_yaml::Value::String("namespace".to_string()),
		serde_yaml::Value::String(namespace.to_string()),
	);

	let mut root = serde_yaml::Mapping::new();
	root.insert(
		serde_yaml::Value::String("apiVersion".to_string()),
		serde_yaml::Value::String("paas.reinhardt-cloud.dev/v1alpha2".to_string()),
	);
	root.insert(
		serde_yaml::Value::String("kind".to_string()),
		serde_yaml::Value::String("ReinhardtApp".to_string()),
	);
	root.insert(
		serde_yaml::Value::String("metadata".to_string()),
		serde_yaml::Value::Mapping(metadata),
	);
	root.insert(
		serde_yaml::Value::String("spec".to_string()),
		serde_yaml::Value::Mapping(spec),
	);

	serde_yaml::Value::Mapping(root)
}

/// Applies YAML content to Kubernetes via `kubectl apply -f -` using async I/O.
///
/// Pipes the YAML content to kubectl's stdin, which avoids temporary files and
/// ensures both production and test code use the same kubectl invocation path.
///
/// When `capture_output` is false, stdout/stderr are inherited so kubectl output
/// streams to the terminal in real-time. When true, output is captured and
/// returned in error messages (useful for testing).
async fn kubectl_apply(
	yaml: &str,
	namespace: &str,
	cluster: Option<&str>,
	capture_output: bool,
) -> Result<(), Box<dyn std::error::Error>> {
	let mut args = vec!["apply", "-f", "-", "-n", namespace];
	if let Some(ctx) = cluster {
		args.extend(["--context", ctx]);
	}

	let (stdout_cfg, stderr_cfg) = if capture_output {
		(std::process::Stdio::piped(), std::process::Stdio::piped())
	} else {
		(
			std::process::Stdio::inherit(),
			std::process::Stdio::inherit(),
		)
	};

	let mut child = tokio::process::Command::new("kubectl")
		.args(&args)
		.stdin(std::process::Stdio::piped())
		.stdout(stdout_cfg)
		.stderr(stderr_cfg)
		.spawn()
		.map_err(|e| format!("failed to run kubectl (is it installed?): {e}"))?;

	if let Some(mut stdin) = child.stdin.take() {
		stdin
			.write_all(yaml.as_bytes())
			.await
			.map_err(|e| format!("failed to write YAML to kubectl stdin: {e}"))?;
		stdin
			.shutdown()
			.await
			.map_err(|e| format!("failed to close kubectl stdin: {e}"))?;
	}

	let output = child
		.wait_with_output()
		.await
		.map_err(|e| format!("failed to wait for kubectl: {e}"))?;

	if output.status.success() {
		Ok(())
	} else {
		let stderr = String::from_utf8_lossy(&output.stderr);
		Err(format!("kubectl apply failed: {stderr}").into())
	}
}

/// Executes the deploy command.
pub(crate) async fn execute(
	args: &DeployArgs,
	client: &ReinhardtCloudClient,
) -> Result<(), Box<dyn std::error::Error>> {
	let span = tracing::info_span!(
		"cli.deploy",
		otel.kind = "client",
		api.version = env!("CARGO_PKG_VERSION"),
		app.name = args.name.as_deref().unwrap_or(""),
		app.namespace = %args.namespace,
	);
	let _enter = span.enter();

	let project_dir = args.dir.clone().unwrap_or_else(|| PathBuf::from("."));

	// Step 1: Try to run manage introspect
	let introspect = match run_manage_introspect() {
		Ok(yaml_output) => {
			if args.introspect_only {
				println!("{yaml_output}");
				return Ok(());
			}
			Some(parse_introspect_output(&yaml_output)?)
		}
		Err(e) => {
			if args.introspect_only {
				return Err(e.into());
			}
			eprintln!("Warning: manage introspect failed: {e}");
			eprintln!("Deploying with minimal configuration.");
			None
		}
	};

	// Step 2: Try to read reinhardt-cloud.toml as a secondary source
	let toml_config = read_reinhardt_cloud_toml(&project_dir)?;

	// Step 3: Determine app name, image, and replicas
	// Priority: CLI args > introspect > reinhardt-cloud.toml > defaults
	let app_name = args
		.name
		.clone()
		.or_else(|| introspect.as_ref().map(|i| i.app.name.clone()))
		.or_else(|| toml_config.as_ref().map(|c| c.app.name.clone()))
		.ok_or("--name is required (or run from a reinhardt project directory)")?;

	let image = args
		.image
		.clone()
		.or_else(|| toml_config.as_ref().map(|c| c.app.image.clone()))
		.ok_or("--image is required")?;

	let replicas = args
		.replicas
		.or_else(|| {
			toml_config
				.as_ref()
				.and_then(|c| c.replicas.as_ref().map(|r| r.count as u32))
		})
		.unwrap_or(1);

	let replicas_i32 = i32::try_from(replicas)
		.map_err(|_| format!("replicas value {replicas} exceeds i32::MAX"))?;

	if toml_config.is_some() && introspect.is_none() {
		println!("Using configuration from reinhardt-cloud.toml");
	}

	// Step 4: Build CRD
	let crd = build_reinhardt_app_crd(
		&app_name,
		&args.namespace,
		&image,
		Some(replicas_i32),
		introspect,
	);

	// Step 5: Output or apply
	if args.dry_run {
		let yaml = serde_yaml::to_string(&crd)?;
		println!("{yaml}");
		return Ok(());
	}

	let yaml = serde_yaml::to_string(&crd)?;

	println!("Deploying {app_name} with image {image} ({replicas} replicas)...");
	if let Some(ref cluster) = args.cluster
		&& !args.direct
	{
		println!("Target cluster: {cluster}");
	}

	if args.direct {
		kubectl_apply(&yaml, &args.namespace, args.cluster.as_deref(), false).await?;
		println!(
			"CRD applied directly to Kubernetes (namespace: {})",
			args.namespace
		);
	} else {
		// API mode: send JSON payload to the dashboard API
		match client
			.deploy(&app_name, &image, args.cluster.as_deref())
			.await
		{
			Ok(response) => {
				println!("Deployment submitted via API.");
				tracing::debug!("API response: {response}");
			}
			Err(e) => {
				return Err(format!("failed to deploy via API: {e}").into());
			}
		}
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use reinhardt_cloud_types::introspect::{
		AppMetadata, DatabaseMetadata, FeaturesMetadata, InfraSignals, MiddlewareMetadata,
		RouteMetadata, SecuritySettings, ServerSettings, SettingsMetadata, TableMetadata,
	};
	use rstest::rstest;

	#[rstest]
	fn test_read_reinhardt_cloud_toml_exists() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(
			dir.path().join("reinhardt-cloud.toml"),
			r#"
[app]
name = "test-app"
image = "test-app:v1"
"#,
		)
		.unwrap();

		// Act
		let result = read_reinhardt_cloud_toml(dir.path());

		// Assert
		let config = result.unwrap().unwrap();
		assert_eq!(config.app.name, "test-app");
		assert_eq!(config.app.image, "test-app:v1");
	}

	#[rstest]
	fn test_read_reinhardt_cloud_toml_missing() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();

		// Act
		let result = read_reinhardt_cloud_toml(dir.path());

		// Assert
		assert!(result.unwrap().is_none());
	}

	#[rstest]
	fn test_read_reinhardt_cloud_toml_malformed_returns_error() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(dir.path().join("reinhardt-cloud.toml"), "invalid {{{ toml").unwrap();

		// Act
		let result = read_reinhardt_cloud_toml(dir.path());

		// Assert
		assert!(result.is_err());
		let err = result.unwrap_err();
		assert!(err.starts_with("Failed to parse reinhardt-cloud.toml:"));
	}

	#[rstest]
	fn test_parse_introspect_yaml() {
		// Arrange
		let yaml = r#"
app:
  name: "my-web-app"
  version: "2.0.0"
databases:
  - alias: "default"
    engine: "postgres"
    tables:
      - name: "users"
        app: "auth"
routes:
  - path: "/api/users/"
    methods: ["GET", "POST"]
    name: "user-list"
middleware:
  - name: "auth"
    type_name: "AuthMiddleware"
settings:
  server:
    default_port: 8080
    debug: false
  security:
    ssl_redirect: true
    session_cookie_secure: true
    csrf_cookie_secure: true
    hsts_enabled: true
features:
  declared: ["database"]
  resolved: ["database", "auth"]
  infrastructure_signals:
    database: "postgres"
    cache: "redis"
    websocket: false
    background_worker: false
    grpc: false
    graphql: false
    admin_panel: true
    i18n: false
"#;

		// Act
		let result = parse_introspect_output(yaml);

		// Assert
		let output = result.unwrap();
		assert_eq!(output.app.name, "my-web-app");
		assert_eq!(output.app.version, "2.0.0");
		assert_eq!(output.databases.len(), 1);
		assert_eq!(output.databases[0].engine, "postgres");
		assert_eq!(output.databases[0].tables.len(), 1);
		assert_eq!(output.databases[0].tables[0].name, "users");
		assert_eq!(output.routes.len(), 1);
		assert_eq!(output.routes[0].path, "/api/users/");
		assert_eq!(output.middleware.len(), 1);
		assert_eq!(output.settings.server.default_port, 8080);
		assert!(output.settings.security.ssl_redirect);
		assert_eq!(output.features.declared, vec!["database"]);
		assert_eq!(
			output.features.infrastructure_signals.database,
			Some("postgres".to_string())
		);
		assert_eq!(
			output.features.infrastructure_signals.cache,
			Some("redis".to_string())
		);
		assert!(output.features.infrastructure_signals.admin_panel);
	}

	#[rstest]
	fn test_parse_introspect_yaml_invalid() {
		// Arrange
		let yaml = "{{invalid yaml:::";

		// Act
		let result = parse_introspect_output(yaml);

		// Assert
		assert!(result.is_err());
		let err = result.unwrap_err();
		assert!(err.starts_with("Failed to parse introspect YAML:"));
	}

	#[rstest]
	fn test_build_reinhardt_app_crd_with_introspect() {
		// Arrange
		let introspect = IntrospectOutput {
			app: AppMetadata {
				name: "my-app".to_string(),
				version: "1.0.0".to_string(),
			},
			databases: vec![DatabaseMetadata {
				alias: "default".to_string(),
				engine: "postgres".to_string(),
				tables: vec![TableMetadata {
					name: "users".to_string(),
					app: "auth".to_string(),
				}],
			}],
			routes: vec![RouteMetadata {
				path: "/api/".to_string(),
				methods: vec!["GET".to_string()],
				name: None,
				namespace: None,
			}],
			middleware: vec![MiddlewareMetadata {
				name: "cors".to_string(),
				type_name: "CorsMiddleware".to_string(),
			}],
			settings: SettingsMetadata {
				server: ServerSettings {
					default_port: 8000,
					debug: false,
				},
				security: SecuritySettings::default(),
			},
			features: FeaturesMetadata {
				declared: vec!["database".to_string()],
				resolved: vec!["database".to_string()],
				infrastructure_signals: InfraSignals {
					database: Some("postgres".to_string()),
					..Default::default()
				},
			},
		};

		// Act
		let crd = build_reinhardt_app_crd(
			"my-app",
			"production",
			"my-app:v1",
			Some(3),
			Some(introspect),
		);

		// Assert
		let mapping = crd.as_mapping().expect("CRD should be a mapping");

		let api_version = mapping
			.get(serde_yaml::Value::String("apiVersion".to_string()))
			.expect("apiVersion should exist");
		assert_eq!(
			api_version,
			&serde_yaml::Value::String("paas.reinhardt-cloud.dev/v1alpha2".to_string())
		);

		let kind = mapping
			.get(serde_yaml::Value::String("kind".to_string()))
			.expect("kind should exist");
		assert_eq!(kind, &serde_yaml::Value::String("ReinhardtApp".to_string()));

		let metadata = mapping
			.get(serde_yaml::Value::String("metadata".to_string()))
			.expect("metadata should exist")
			.as_mapping()
			.expect("metadata should be mapping");
		assert_eq!(
			metadata.get(serde_yaml::Value::String("name".to_string())),
			Some(&serde_yaml::Value::String("my-app".to_string()))
		);
		assert_eq!(
			metadata.get(serde_yaml::Value::String("namespace".to_string())),
			Some(&serde_yaml::Value::String("production".to_string()))
		);

		let spec = mapping
			.get(serde_yaml::Value::String("spec".to_string()))
			.expect("spec should exist")
			.as_mapping()
			.expect("spec should be mapping");
		assert_eq!(
			spec.get(serde_yaml::Value::String("image".to_string())),
			Some(&serde_yaml::Value::String("my-app:v1".to_string()))
		);
		assert_eq!(
			spec.get(serde_yaml::Value::String("replicas".to_string())),
			Some(&serde_yaml::Value::Number(serde_yaml::Number::from(3)))
		);

		// Verify introspect is present
		let introspect_value = spec
			.get(serde_yaml::Value::String("introspect".to_string()))
			.expect("introspect should exist in spec");
		assert!(introspect_value.as_mapping().is_some());
	}

	#[rstest]
	fn test_build_reinhardt_app_crd_without_introspect() {
		// Arrange & Act
		let crd = build_reinhardt_app_crd("simple-app", "default", "simple:latest", Some(1), None);

		// Assert
		let mapping = crd.as_mapping().expect("CRD should be a mapping");

		let api_version = mapping
			.get(serde_yaml::Value::String("apiVersion".to_string()))
			.expect("apiVersion should exist");
		assert_eq!(
			api_version,
			&serde_yaml::Value::String("paas.reinhardt-cloud.dev/v1alpha2".to_string())
		);

		let kind = mapping
			.get(serde_yaml::Value::String("kind".to_string()))
			.expect("kind should exist");
		assert_eq!(kind, &serde_yaml::Value::String("ReinhardtApp".to_string()));

		let spec = mapping
			.get(serde_yaml::Value::String("spec".to_string()))
			.expect("spec should exist")
			.as_mapping()
			.expect("spec should be mapping");
		assert_eq!(
			spec.get(serde_yaml::Value::String("image".to_string())),
			Some(&serde_yaml::Value::String("simple:latest".to_string()))
		);
		assert_eq!(
			spec.get(serde_yaml::Value::String("replicas".to_string())),
			Some(&serde_yaml::Value::Number(serde_yaml::Number::from(1)))
		);

		// Verify introspect is absent
		assert!(
			spec.get(serde_yaml::Value::String("introspect".to_string()))
				.is_none()
		);
	}

	#[rstest]
	#[tokio::test]
	async fn test_kubectl_apply_writes_valid_yaml() {
		// Arrange
		let crd = build_reinhardt_app_crd("test-app", "staging", "test:v1", Some(2), None);
		let yaml = serde_yaml::to_string(&crd).unwrap();

		// Act: call kubectl_apply - kubectl is not available in CI,
		// so we expect an error about kubectl not being found or apply failing.
		let result = kubectl_apply(&yaml, "staging", None, true).await;

		// Assert: the function should return an error (kubectl not available in test env),
		// but the error message should indicate kubectl execution, not YAML serialization.
		assert!(result.is_err());
		let err = result.unwrap_err().to_string();
		assert!(
			err.contains("kubectl") || err.contains("apply"),
			"expected kubectl-related error, got: {err}"
		);
	}

	#[rstest]
	#[tokio::test]
	async fn test_kubectl_apply_passes_cluster_context() {
		// Arrange
		let crd = build_reinhardt_app_crd("ctx-app", "prod", "ctx:v1", Some(1), None);
		let yaml = serde_yaml::to_string(&crd).unwrap();

		// Act
		let result = kubectl_apply(&yaml, "prod", Some("my-cluster"), true).await;

		// Assert: should fail with kubectl error, not a code-level error
		assert!(result.is_err());
		let err = result.unwrap_err().to_string();
		assert!(
			err.contains("kubectl") || err.contains("apply"),
			"expected kubectl-related error, got: {err}"
		);
	}
}
