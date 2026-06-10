//! Deploy command: deploys an application to the Reinhardt Cloud platform.

use clap::Args;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;

use crate::client::ReinhardtCloudClient;
use crate::crd_version::{COMPILE_TIME_DEFAULT, resolve_api_version};
use crate::feature_detector::{InfraSignals as DetectedInfraSignals, detect_project};
use crate::settings_reader::{DatabaseConfig, read_database_config};
use reinhardt_cloud_core::infrastructure_derivation::{
	InfrastructureDerivationInput, derive_infrastructure_spec,
};
use reinhardt_cloud_types::crd::ProjectSpec;
use reinhardt_cloud_types::introspect::{
	AppMetadata, DatabaseMetadata, FeaturesMetadata, InfraSignals as IntrospectInfraSignals,
	IntrospectOutput,
};
use reinhardt_cloud_types::reinhardt_cloud_toml::ReinhardtCloudToml;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

const INTROSPECT_TIMEOUT_ENV: &str = "REINHARDT_CLOUD_DEPLOY_INTROSPECT_TIMEOUT_SECONDS";

/// Deploy an application.
#[derive(Debug, Args)]
pub(crate) struct DeployArgs {
	/// Project name (overrides reinhardt-cloud.toml if set)
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

	/// Path to the project manage binary used for introspection
	#[arg(long)]
	pub manage_bin: Option<PathBuf>,

	/// Fail deploy when manage introspect is unavailable instead of using zero-config fallback
	#[arg(long)]
	pub require_introspect: bool,

	/// Kubernetes namespace
	#[arg(long, default_value = "default")]
	pub namespace: String,

	/// Target cluster name
	#[arg(long)]
	pub cluster: Option<String>,

	/// Override the apiVersion field in the generated `Project` manifest
	/// (e.g. `paas.reinhardt-cloud.dev/v1`). Only meaningful in `--direct`
	/// mode (where the manifest is applied to the cluster) or with `--dry-run`
	/// (where the manifest is printed for review). Default API mode is
	/// unsupported after the dashboard moved deploy submissions to server
	/// functions. When unset and `--direct` is used, the CLI queries the
	/// cluster's CRD and selects the served storage version automatically.
	/// The value MUST be a fully-qualified `group/version` — short forms like
	/// `v1` are rejected so we never produce manifests with a missing API group.
	#[arg(long, value_parser = validate_api_version)]
	pub api_version: Option<String>,
}

/// Reject `--api-version` values that are not fully-qualified
/// `group/version` strings (e.g. `paas.reinhardt-cloud.dev/v1`). Short
/// forms like `v1` would silently produce manifests missing the API group,
/// which the cluster then rejects with an opaque error far from the cause.
fn validate_api_version(value: &str) -> Result<String, String> {
	let mut parts = value.split('/');
	let group = parts.next().unwrap_or_default();
	let version = parts.next().unwrap_or_default();

	if group.is_empty() || version.is_empty() || parts.next().is_some() {
		return Err(format!(
			"invalid value for --api-version: expected fully-qualified group/version (for example `paas.reinhardt-cloud.dev/v1`), got `{value}`"
		));
	}

	// Reject segments that are whitespace-only or contain embedded whitespace
	// (e.g. "  /v1" or "group/v 1"). Such values pass the emptiness check but
	// would silently produce malformed apiVersion strings.
	let group_trimmed = group.trim();
	let version_trimmed = version.trim();
	if group_trimmed.is_empty()
		|| version_trimmed.is_empty()
		|| group.contains(char::is_whitespace)
		|| version.contains(char::is_whitespace)
	{
		return Err(format!(
			"invalid value for --api-version: group and version must not contain whitespace, got `{value}`"
		));
	}

	Ok(value.to_string())
}

/// Converts CLI-detected InfraSignals into the IntrospectOutput shape.
///
/// This is the zero-config inference path (Refs #372): when the management
/// server is not reachable, we still want the deploy pipeline to receive a
/// best-effort `IntrospectOutput` synthesized from local project state.
///
/// All fields of `DetectedInfraSignals` are read here so the struct remains
/// a stable contract — add new mappings in lockstep when new signals land.
fn synthesize_infra_signals(detected: &DetectedInfraSignals) -> IntrospectInfraSignals {
	// Bind build-time-only signals up front so the compiler sees every field
	// of `DetectedInfraSignals` as read. They are deliberately omitted from
	// the IntrospectOutput shape returned below — see the trailing comment
	// for the per-field rationale.
	let _ = detected.protoc_needed;

	IntrospectInfraSignals {
		database: detected.database.clone(),
		cache: detected.cache.clone(),
		websocket: detected.websocket,
		background_worker: detected.background_worker,
		grpc: detected.grpc,
		storage: detected.object_storage.then(|| "local".to_string()),
		mail: None,
		// Map the `sessions` boolean to a backend hint so downstream can tell
		// sessions are enabled; default to "db" because that matches the
		// reinhardt-web built-in session backend shipped without configuration.
		session_backend: detected.sessions.then(|| "db".to_string()),
		graphql: detected.graphql,
		admin_panel: false,
		i18n: false,
		pages: detected.pages,
	}
	// Note: `detected.jwt` is intentionally not surfaced here because the
	// IntrospectOutput schema has no JWT-specific field. JWT usage affects
	// RBAC manifests generated later, which is outside the zero-config path.
	//
	// Note: `detected.protoc_needed` is intentionally not surfaced here
	// because it is a build-time concern (driving Dockerfile generation,
	// not deploy-time runtime topology). The IntrospectOutput schema
	// describes the running app, not its build prerequisites.
}

/// Builds a synthetic `IntrospectOutput` from a project directory.
///
/// Uses `feature_detector::detect_project` for app identity and feature
/// signals, and `settings_reader::read_database_config` for the default
/// database configuration. Returns `None` when `detect_project` fails
/// (e.g., no `Cargo.toml` found); a missing database config is not
/// sufficient to return `None` — app identity must be resolvable.
fn synthesize_introspect_from_project(dir: &std::path::Path) -> Option<IntrospectOutput> {
	let project = detect_project(dir).ok()?;
	let db_config: Option<DatabaseConfig> = read_database_config(dir);

	if let Some(ref cfg) = db_config {
		// Log only the engine type; host, port, and name are omitted to
		// avoid leaking connection details into shared CI logs.
		eprintln!(
			"  using inferred database configuration (engine={})",
			cfg.engine
		);
	}

	let databases = db_config
		.map(|cfg| {
			vec![DatabaseMetadata {
				alias: "default".to_string(),
				engine: cfg.engine,
				tables: Vec::new(),
			}]
		})
		.unwrap_or_default();

	Some(IntrospectOutput {
		app: AppMetadata {
			name: project.name,
			version: project.version,
		},
		databases,
		routes: Vec::new(),
		middleware: Vec::new(),
		settings: Default::default(),
		features: FeaturesMetadata {
			declared: project.features.clone(),
			resolved: project.features,
			infrastructure_signals: synthesize_infra_signals(&project.signals),
		},
	})
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

fn manage_introspect_command(manage_bin: &Path, project_dir: &Path) -> Command {
	let mut command = Command::new(manage_bin);
	command
		.args(["introspect", "--format", "yaml"])
		.current_dir(project_dir);
	command
}

fn resolve_manage_bin_path(manage_bin: &Path) -> PathBuf {
	if manage_bin.is_absolute() {
		manage_bin.to_path_buf()
	} else {
		std::env::current_dir()
			.unwrap_or_else(|_| PathBuf::from("."))
			.join(manage_bin)
	}
}

fn project_manage_bin(project_dir: &Path) -> Option<PathBuf> {
	let candidate = project_dir.join("manage");
	if !candidate.is_file() {
		return None;
	}
	Some(candidate.canonicalize().unwrap_or(candidate))
}

fn path_manage_introspect_command(project_dir: &Path) -> Option<Command> {
	project_manage_bin(project_dir)
		.as_ref()
		.map(|manage_bin| manage_introspect_command(manage_bin, project_dir))
}

fn cargo_manage_introspect_command(project_dir: &Path) -> Command {
	let mut command = Command::new("cargo");
	command
		.args([
			"run",
			"--bin",
			"manage",
			"--",
			"introspect",
			"--format",
			"yaml",
		])
		.current_dir(project_dir);
	command
}

fn parse_introspect_timeout_seconds(raw: &str) -> Option<Duration> {
	let seconds = raw.parse::<u64>().ok()?;
	if seconds == 0 {
		return None;
	}
	Some(Duration::from_secs(seconds))
}

fn introspect_timeout() -> Option<Duration> {
	std::env::var(INTROSPECT_TIMEOUT_ENV)
		.ok()
		.and_then(|raw| parse_introspect_timeout_seconds(&raw))
}

fn should_fail_on_introspect_error(args: &DeployArgs) -> bool {
	args.introspect_only || args.require_introspect
}

fn terminate_child(child: &mut std::process::Child) {
	#[cfg(unix)]
	{
		let process_group = format!("-{}", child.id());
		let _ = Command::new("kill")
			.args(["-TERM", &process_group])
			.status();
		for _ in 0..20 {
			if matches!(child.try_wait(), Ok(Some(_))) {
				return;
			}
			thread::sleep(Duration::from_millis(50));
		}
		let _ = Command::new("kill")
			.args(["-KILL", &process_group])
			.status();
	}

	#[cfg(not(unix))]
	{
		let _ = child.kill();
	}

	let _ = child.wait();
}

fn command_output_with_optional_timeout(
	mut command: Command,
	label: &str,
	timeout: Option<Duration>,
) -> Result<Output, String> {
	if let Some(timeout) = timeout {
		command.stdout(Stdio::piped()).stderr(Stdio::piped());
		#[cfg(unix)]
		{
			command.process_group(0);
		}

		let mut child = command
			.spawn()
			.map_err(|e| format!("Failed to run {label}: {e}"))?;
		let started = Instant::now();
		loop {
			match child.try_wait() {
				Ok(Some(_)) => {
					return child
						.wait_with_output()
						.map_err(|e| format!("Failed to collect {label} output: {e}"));
				}
				Ok(None) if started.elapsed() >= timeout => {
					terminate_child(&mut child);
					return Err(format!(
						"{label} timed out after {} seconds",
						timeout.as_secs()
					));
				}
				Ok(None) => thread::sleep(Duration::from_millis(100)),
				Err(e) => return Err(format!("Failed to wait for {label}: {e}")),
			}
		}
	}

	command
		.output()
		.map_err(|e| format!("Failed to run {label}: {e}"))
}

/// Runs `manage introspect --format yaml` and returns stdout.
///
/// Tries the production binary first, then falls back to
/// `cargo run -- introspect --format yaml` for development mode.
fn run_manage_introspect(project_dir: &Path, manage_bin: Option<&Path>) -> Result<String, String> {
	let timeout = introspect_timeout();

	if let Some(manage_bin) = manage_bin {
		let manage_bin = resolve_manage_bin_path(manage_bin);
		let output = command_output_with_optional_timeout(
			manage_introspect_command(&manage_bin, project_dir),
			"manage introspect",
			timeout,
		)?;
		return if output.status.success() {
			String::from_utf8(output.stdout)
				.map_err(|e| format!("Invalid UTF-8 in manage output: {e}"))
		} else {
			let stderr = String::from_utf8_lossy(&output.stderr);
			Err(format!("manage introspect failed: {stderr}"))
		};
	}

	if let Some(command) = path_manage_introspect_command(project_dir) {
		let output = command_output_with_optional_timeout(command, "manage introspect", timeout)?;
		return if output.status.success() {
			String::from_utf8(output.stdout)
				.map_err(|e| format!("Invalid UTF-8 in manage output: {e}"))
		} else {
			let stderr = String::from_utf8_lossy(&output.stderr);
			Err(format!("manage introspect failed: {stderr}"))
		};
	}

	let result = command_output_with_optional_timeout(
		manage_introspect_command(Path::new("manage"), project_dir),
		"manage introspect",
		timeout,
	);

	if let Ok(output) = result
		&& output.status.success()
	{
		return String::from_utf8(output.stdout)
			.map_err(|e| format!("Invalid UTF-8 in manage output: {e}"));
	}

	// Fall back to cargo run for development mode
	let result = command_output_with_optional_timeout(
		cargo_manage_introspect_command(project_dir),
		"cargo manage introspect",
		timeout,
	)?;

	if result.status.success() {
		String::from_utf8(result.stdout).map_err(|e| format!("Invalid UTF-8 in cargo output: {e}"))
	} else {
		let stderr = String::from_utf8_lossy(&result.stderr);
		Err(format!("manage introspect failed: {stderr}"))
	}
}

/// Builds a `ProjectSpec` from `reinhardt-cloud.toml`, CLI overrides,
/// and optional introspect data.
fn build_project_spec(
	toml_config: Option<&ReinhardtCloudToml>,
	project_name: &str,
	image: String,
	replicas: i32,
	introspect: Option<IntrospectOutput>,
) -> Result<ProjectSpec, String> {
	let mut spec = toml_config
		.map(ReinhardtCloudToml::to_project_spec)
		.unwrap_or_else(|| ProjectSpec {
			image: image.clone(),
			replicas: Some(replicas),
			..Default::default()
		});

	spec.image = image;
	spec.replicas = Some(replicas);
	spec.introspect = introspect;
	if spec.infrastructure.is_none()
		&& let Some(introspect) = spec.introspect.as_ref()
	{
		spec.infrastructure = derive_infrastructure_spec(InfrastructureDerivationInput {
			project_name: project_name.to_string(),
			signals: introspect.features.infrastructure_signals.clone(),
			explicit: None,
			typed_secret_refs: typed_secret_refs(&spec),
		})
		.map_err(|e| e.to_string())?;
	}

	Ok(spec)
}

fn typed_secret_refs(spec: &ProjectSpec) -> Vec<String> {
	[
		spec.auth
			.as_ref()
			.and_then(|auth| auth.oauth.as_ref())
			.and_then(|oauth| oauth.credentials_secret.as_ref()),
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

/// Removes null values from serialized Kubernetes YAML so absent optional
/// fields stay absent in dry-run output and server-side apply payloads.
fn prune_yaml_nulls(value: &mut serde_yaml::Value) {
	match value {
		serde_yaml::Value::Mapping(mapping) => {
			mapping.retain(|_, nested| !nested.is_null());
			for nested in mapping.values_mut() {
				prune_yaml_nulls(nested);
			}
		}
		serde_yaml::Value::Sequence(items) => {
			for item in items {
				prune_yaml_nulls(item);
			}
		}
		_ => {}
	}
}

/// Builds a `Project` CRD YAML value with typed spec data.
fn build_project_crd(
	name: &str,
	namespace: &str,
	spec: &ProjectSpec,
	api_version: &str,
) -> serde_yaml::Value {
	let mut spec_value = serde_yaml::to_value(spec).unwrap_or(serde_yaml::Value::Null);
	prune_yaml_nulls(&mut spec_value);

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
		serde_yaml::Value::String(api_version.to_string()),
	);
	root.insert(
		serde_yaml::Value::String("kind".to_string()),
		serde_yaml::Value::String("Project".to_string()),
	);
	root.insert(
		serde_yaml::Value::String("metadata".to_string()),
		serde_yaml::Value::Mapping(metadata),
	);
	root.insert(serde_yaml::Value::String("spec".to_string()), spec_value);

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
	use tracing::Instrument;
	let span = tracing::info_span!(
		"cli.deploy",
		otel.kind = "client",
		cli.version = env!("CARGO_PKG_VERSION"),
		app.name = args.name.as_deref().unwrap_or(""),
		app.namespace = %args.namespace,
	);
	execute_inner(args, client).instrument(span).await
}

async fn execute_inner(
	args: &DeployArgs,
	client: &ReinhardtCloudClient,
) -> Result<(), Box<dyn std::error::Error>> {
	eprintln!("Target: {}", client.base_url());
	let project_dir = args.dir.clone().unwrap_or_else(|| PathBuf::from("."));
	if !args.dry_run && !args.direct && !args.introspect_only {
		return Err(unsupported_dashboard_deploy_error());
	}

	// Step 1: Try to run manage introspect
	let introspect = match run_manage_introspect(&project_dir, args.manage_bin.as_deref()) {
		Ok(yaml_output) => {
			if args.introspect_only {
				println!("{yaml_output}");
				return Ok(());
			}
			Some(parse_introspect_output(&yaml_output)?)
		}
		Err(e) => {
			if should_fail_on_introspect_error(args) {
				return Err(e.into());
			}
			eprintln!("Warning: manage introspect failed: {e}");
			// Zero-config fallback (Refs #372): when `manage introspect` is
			// unavailable, infer project metadata from Cargo.toml feature
			// flags and settings/base.toml so deploy still produces a usable
			// CRD before the management server is reachable.
			match synthesize_introspect_from_project(&project_dir) {
				Some(synthesized) => {
					eprintln!(
						"Using zero-config inference from Cargo.toml and settings/base.toml."
					);
					Some(synthesized)
				}
				None => {
					eprintln!("Deploying with minimal configuration.");
					None
				}
			}
		}
	};

	// Step 2: Try to read reinhardt-cloud.toml as a secondary source
	let toml_config = read_reinhardt_cloud_toml(&project_dir)?;

	// Step 3: Determine app name, image, and replicas
	// Priority: CLI args > introspect > reinhardt-cloud.toml > defaults
	let project_name = args
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

	// Step 4: Resolve apiVersion.
	//
	// For `--direct` deploys, query the target cluster's CRD so the CLI
	// stays compatible with whatever served version the operator exposes.
	// Otherwise (API-mode deploys, dry-run), fall back to the compile-time
	// default because no cluster is contacted.
	//
	// `--dry-run` must not contact the cluster even when `--direct` is set;
	// treat dry-run as non-direct for version discovery so the preview stays
	// fully offline.
	let api_version = resolve_deploy_api_version(
		args.direct && !args.dry_run,
		args.api_version.as_deref(),
		args.cluster.as_deref(),
	)
	.await?;

	// Step 5: Build typed spec and CRD
	let spec = build_project_spec(
		toml_config.as_ref(),
		&project_name,
		image.clone(),
		replicas_i32,
		introspect,
	)?;
	if let Err(errors) = spec.validate() {
		let messages = errors
			.into_iter()
			.map(|e| e.message)
			.collect::<Vec<_>>()
			.join("; ");
		return Err(format!("invalid Project spec: {messages}").into());
	}
	let crd = build_project_crd(&project_name, &args.namespace, &spec, &api_version);

	// Step 6: Output or apply
	if args.dry_run {
		let yaml = serde_yaml::to_string(&crd)?;
		println!("{yaml}");
		return Ok(());
	}

	let yaml = serde_yaml::to_string(&crd)?;

	println!("Deploying {project_name} with image {image} ({replicas} replicas)...");
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
		let _ = yaml;
		return Err(unsupported_dashboard_deploy_error());
	}

	Ok(())
}

fn unsupported_dashboard_deploy_error() -> Box<dyn std::error::Error> {
	"default deploy via dashboard REST is no longer supported; use --direct or --dry-run. \
	The dashboard Pages UI submits deployments through server functions."
		.into()
}

/// Decide which apiVersion to embed in the generated `Project` CRD.
///
/// Selection priority:
/// 1. Non-`--direct` invocations never contact a cluster, so an explicit
///    override (or the compile-time default) is used directly. This keeps
///    dry-runs and API-mode deploys offline.
/// 2. `--direct` with an explicit `--api-version` short-circuits cluster
///    discovery — the user has already pinned the version they want.
/// 3. `--direct` without an override builds a kube `Client` honoring the
///    `--cluster` (kubeconfig context) flag so discovery and the later
///    `kubectl --context` apply target the *same* cluster, then queries
///    the live CRD via `resolve_api_version`.
///
/// Extracted from `execute` so the selection logic is testable without
/// requiring a live kubeconfig or cluster.
async fn resolve_deploy_api_version(
	direct: bool,
	explicit_api_version: Option<&str>,
	cluster_context: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
	if !direct {
		return Ok(explicit_api_version
			.map(str::to_owned)
			.unwrap_or_else(|| COMPILE_TIME_DEFAULT.to_string()));
	}

	if let Some(explicit) = explicit_api_version {
		return Ok(explicit.to_string());
	}

	// Known limitation: when no --cluster context is given, kube::Client::try_default()
	// uses inferred Kubernetes configuration (for example, in-cluster config or the
	// current kubeconfig context), which may differ from the cluster that kubectl
	// will target when the apply runs. To guarantee both discovery and apply hit
	// the same cluster, pass --cluster explicitly.
	if cluster_context.is_none() {
		eprintln!(
			"Warning: --direct is set without --cluster; apiVersion discovery uses \
			 inferred Kubernetes config (for example, in-cluster config or the current \
			 kubeconfig context), which may differ from the kubectl apply target. \
			 Pass --cluster to pin both discovery and apply to the same cluster."
		);
	}

	let kube_client_result = match cluster_context {
		Some(context) => {
			let opts = kube::config::KubeConfigOptions {
				context: Some(context.to_string()),
				..Default::default()
			};
			match kube::Config::from_kubeconfig(&opts).await {
				Ok(config) => kube::Client::try_from(config),
				Err(e) => Err(e.into()),
			}
		}
		None => kube::Client::try_default().await,
	};

	let kube_client = kube_client_result.map_err(|e| -> Box<dyn std::error::Error> {
		format!("failed to build Kubernetes client for apiVersion discovery: {e} (pass --api-version <group/version> to skip cluster discovery when running without a kubeconfig or in-cluster config)").into()
	})?;

	resolve_api_version(&kube_client, None).await
}

#[cfg(test)]
mod tests {
	use super::*;
	use reinhardt_cloud_types::crd::{
		AuthSpec, MailSpec, OAuthSpec, SourceSpec, WebhookEvent, WebhookSpec,
	};
	use reinhardt_cloud_types::introspect::{
		AppMetadata, DatabaseMetadata, FeaturesMetadata, InfraSignals, MiddlewareMetadata,
		RouteMetadata, SecuritySettings, ServerSettings, SettingsMetadata, TableMetadata,
	};
	use reinhardt_cloud_types::reinhardt_cloud_toml::{
		AppSection, DatabaseSection, HealthSection, ReinhardtCloudToml, ReplicasSection,
		ScaleSection, ServicesSection,
	};
	use rstest::rstest;

	#[rstest]
	#[case::positive("30", Some(Duration::from_secs(30)))]
	#[case::zero_disabled("0", None)]
	#[case::invalid("abc", None)]
	fn test_parse_introspect_timeout_seconds(
		#[case] raw: &str,
		#[case] expected: Option<Duration>,
	) {
		assert_eq!(parse_introspect_timeout_seconds(raw), expected);
	}

	fn introspect_with_infra_signals(project_name: &str, signals: InfraSignals) -> IntrospectOutput {
		IntrospectOutput {
			app: AppMetadata {
				name: project_name.to_string(),
				version: "1.0.0".to_string(),
			},
			features: FeaturesMetadata {
				infrastructure_signals: signals,
				..Default::default()
			},
			..Default::default()
		}
	}

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
	fn test_manage_introspect_commands_use_project_dir() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		let manage_bin = dir.path().join("custom-manage");

		// Act
		let manage = manage_introspect_command(&manage_bin, dir.path());
		let cargo = cargo_manage_introspect_command(dir.path());

		// Assert
		assert_eq!(manage.get_current_dir(), Some(dir.path()));
		assert_eq!(cargo.get_current_dir(), Some(dir.path()));
		assert_eq!(manage.get_program(), manage_bin.as_os_str());
		let manage_args: Vec<String> = manage
			.get_args()
			.map(|arg| arg.to_string_lossy().into_owned())
			.collect();
		assert_eq!(manage_args, vec!["introspect", "--format", "yaml"]);
		let cargo_args: Vec<String> = cargo
			.get_args()
			.map(|arg| arg.to_string_lossy().into_owned())
			.collect();
		assert_eq!(
			cargo_args,
			vec![
				"run",
				"--bin",
				"manage",
				"--",
				"introspect",
				"--format",
				"yaml"
			]
		);
	}

	#[rstest]
	fn test_path_manage_introspect_command_prefers_project_manage_binary() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		let manage_bin = dir.path().join("manage");
		std::fs::write(&manage_bin, "").unwrap();
		let expected_manage_bin = manage_bin.canonicalize().unwrap();

		// Act
		let command = path_manage_introspect_command(dir.path())
			.expect("project manage binary should be detected");

		// Assert
		assert_eq!(command.get_current_dir(), Some(dir.path()));
		assert_eq!(command.get_program(), expected_manage_bin.as_os_str());
		let args: Vec<String> = command
			.get_args()
			.map(|arg| arg.to_string_lossy().into_owned())
			.collect();
		assert_eq!(args, vec!["introspect", "--format", "yaml"]);
	}

	#[rstest]
	fn test_path_manage_introspect_command_absent_without_project_manage_binary() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();

		// Act
		let command = path_manage_introspect_command(dir.path());

		// Assert
		assert!(command.is_none());
	}

	#[rstest]
	fn test_resolve_manage_bin_path_absolutizes_relative_paths() {
		// Arrange
		let manage_bin = Path::new("target/debug/manage");

		// Act
		let resolved = resolve_manage_bin_path(manage_bin);

		// Assert
		assert!(resolved.is_absolute());
		assert!(resolved.ends_with(manage_bin));
	}

	#[rstest]
	fn test_resolve_manage_bin_path_preserves_absolute_paths() {
		// Arrange
		let manage_bin = Path::new("/tmp/reinhardt-cloud-manage");

		// Act
		let resolved = resolve_manage_bin_path(manage_bin);

		// Assert
		assert_eq!(resolved, manage_bin);
	}

	#[rstest]
	#[case::regular_deploy(false, false, false)]
	#[case::introspect_only(true, false, true)]
	#[case::require_introspect(false, true, true)]
	#[case::both(true, true, true)]
	fn test_should_fail_on_introspect_error(
		#[case] introspect_only: bool,
		#[case] require_introspect: bool,
		#[case] expected: bool,
	) {
		// Arrange
		let args = DeployArgs {
			name: None,
			image: None,
			replicas: None,
			dir: None,
			dry_run: false,
			direct: false,
			introspect_only,
			manage_bin: None,
			require_introspect,
			namespace: "default".to_string(),
			cluster: None,
			api_version: None,
		};

		// Act
		let actual = should_fail_on_introspect_error(&args);

		// Assert
		assert_eq!(actual, expected);
	}

	#[rstest]
	#[tokio::test]
	async fn test_default_deploy_returns_unsupported_before_project_inspection() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		let args = DeployArgs {
			name: None,
			image: None,
			replicas: None,
			dir: Some(dir.path().to_path_buf()),
			dry_run: false,
			direct: false,
			introspect_only: false,
			manage_bin: None,
			require_introspect: true,
			namespace: "default".to_string(),
			cluster: None,
			api_version: None,
		};
		let client = ReinhardtCloudClient::new("http://localhost:8000").unwrap();

		// Act
		let error = execute_inner(&args, &client)
			.await
			.expect_err("default dashboard REST deploy should be unsupported");

		// Assert
		assert_eq!(
			error.to_string(),
			"default deploy via dashboard REST is no longer supported; use --direct or --dry-run. The dashboard Pages UI submits deployments through server functions."
		);
	}

	#[rstest]
	fn test_build_project_spec_derives_infrastructure_from_introspect() {
		// Arrange
		let introspect = introspect_with_infra_signals(
			"orders",
			InfraSignals {
				database: Some("postgres".to_string()),
				storage: Some("s3".to_string()),
				..Default::default()
			},
		);

		// Act
		let spec =
			build_project_spec(None, "orders", "orders:v1".to_string(), 2, Some(introspect))
				.expect("spec should build");

		// Assert
		let infrastructure = spec
			.infrastructure
			.expect("infrastructure should be derived");
		assert!(infrastructure.postgres.is_some());
		let buckets = infrastructure.buckets.expect("bucket should be derived");
		assert_eq!(buckets.len(), 1);
		assert_eq!(buckets[0].name, "orders-assets");
	}

	#[rstest]
	fn test_build_project_spec_uses_resolved_project_name_for_infrastructure() {
		// Arrange
		let introspect = introspect_with_infra_signals(
			"introspected-name",
			InfraSignals {
				storage: Some("s3".to_string()),
				..Default::default()
			},
		);

		// Act
		let spec = build_project_spec(
			None,
			"cli-name",
			"orders:v1".to_string(),
			2,
			Some(introspect),
		)
		.expect("spec should build");

		// Assert
		let infrastructure = spec
			.infrastructure
			.expect("infrastructure should be derived");
		let buckets = infrastructure.buckets.expect("bucket should be derived");
		assert_eq!(buckets[0].name, "cli-name-assets");
	}

	#[rstest]
	fn test_typed_secret_refs_collects_crd_secret_refs() {
		// Arrange
		let spec = ProjectSpec {
			image: "orders:v1".to_string(),
			auth: Some(AuthSpec {
				jwt: true,
				oauth: Some(OAuthSpec {
					provider: "github".to_string(),
					credentials_secret: Some("oauth-creds".to_string()),
				}),
			}),
			source: Some(SourceSpec {
				repository: "https://github.com/example/orders".to_string(),
				branch: None,
				provider: None,
				credentials_secret: Some("git-creds".to_string()),
				build: None,
				webhook: Some(WebhookSpec {
					enabled: true,
					secret_ref: Some("webhook-secret".to_string()),
					events: vec![WebhookEvent::Push],
				}),
				preview: None,
			}),
			mail: Some(MailSpec {
				smtp_host: Some("smtp.example.com".to_string()),
				smtp_port: Some(587),
				credentials_secret: Some("mail-creds".to_string()),
			}),
			..Default::default()
		};

		// Act
		let refs = typed_secret_refs(&spec);

		// Assert
		assert_eq!(
			refs,
			vec![
				"oauth-creds".to_string(),
				"git-creds".to_string(),
				"webhook-secret".to_string(),
				"mail-creds".to_string()
			]
		);
	}

	#[rstest]
	fn test_build_project_spec_fails_on_unsupported_storage() {
		// Arrange
		let introspect = introspect_with_infra_signals(
			"orders",
			InfraSignals {
				storage: Some("local".to_string()),
				..Default::default()
			},
		);

		// Act
		let result =
			build_project_spec(None, "orders", "orders:v1".to_string(), 2, Some(introspect));

		// Assert
		let error = result.expect_err("unsupported storage should fail");
		assert!(
			error.contains("unsupported managed storage backend `local`"),
			"unexpected error: {error}"
		);
	}

	#[rstest]
	fn test_build_project_crd_with_introspect() {
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
		let spec =
			build_project_spec(None, "my-app", "my-app:v1".to_string(), 3, Some(introspect))
				.expect("spec should build");
		let crd = build_project_crd(
			"my-app",
			"production",
			&spec,
			"paas.reinhardt-cloud.dev/v1alpha2",
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
		assert_eq!(kind, &serde_yaml::Value::String("Project".to_string()));

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
	fn test_build_project_crd_without_introspect() {
		// Arrange
		let spec =
			build_project_spec(None, "simple-app", "simple:latest".to_string(), 1, None)
				.expect("spec should build");

		// Act
		let crd = build_project_crd(
			"simple-app",
			"default",
			&spec,
			"paas.reinhardt-cloud.dev/v1alpha2",
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
		assert_eq!(kind, &serde_yaml::Value::String("Project".to_string()));

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
	fn test_build_project_crd_preserves_typed_toml_sections() {
		// Arrange
		let mut config = ReinhardtCloudToml {
			app: AppSection {
				name: "dashboard".to_string(),
				image: "dashboard:latest".to_string(),
			},
			database: Some(DatabaseSection {
				engine: "postgresql".to_string(),
				version: Some("16".to_string()),
				..Default::default()
			}),
			health: Some(HealthSection {
				path: Some("/api/healthz/".to_string()),
				port: Some(8000),
				interval_seconds: Some(10),
			}),
			services: Some(ServicesSection {
				port: Some(80),
				target_port: Some(8000),
				ingress_host: None,
			}),
			replicas: Some(ReplicasSection { count: 2 }),
			scale: Some(ScaleSection {
				min_replicas: Some(2),
				max_replicas: Some(6),
				metric: Some("cpu".to_string()),
				target_value: Some(70),
			}),
			..Default::default()
		};
		config.env.insert(
			"DATABASE_URL".to_string(),
			"secretRef:dashboard/database-url".to_string(),
		);

		// Act
		let spec = build_project_spec(
			Some(&config),
			"dashboard",
			"dashboard:v1".to_string(),
			2,
			None,
		)
		.expect("spec should build");
		let crd = build_project_crd(
			"dashboard",
			"production",
			&spec,
			"paas.reinhardt-cloud.dev/v1alpha2",
		);

		// Assert
		let spec = crd
			.as_mapping()
			.unwrap()
			.get(serde_yaml::Value::String("spec".to_string()))
			.unwrap()
			.as_mapping()
			.unwrap();
		assert_eq!(
			spec.get(serde_yaml::Value::String("image".to_string())),
			Some(&serde_yaml::Value::String("dashboard:v1".to_string()))
		);
		assert!(
			spec.get(serde_yaml::Value::String("database".to_string()))
				.is_some()
		);
		assert!(
			spec.get(serde_yaml::Value::String("health".to_string()))
				.is_some()
		);
		assert!(
			spec.get(serde_yaml::Value::String("services".to_string()))
				.is_some()
		);
		assert!(
			spec.get(serde_yaml::Value::String("scale".to_string()))
				.is_some()
		);
		let env = spec
			.get(serde_yaml::Value::String("env".to_string()))
			.unwrap()
			.as_mapping()
			.unwrap();
		assert_eq!(
			env.get(serde_yaml::Value::String("DATABASE_URL".to_string())),
			Some(&serde_yaml::Value::String(
				"secretRef:dashboard/database-url".to_string()
			))
		);
	}

	#[rstest]
	#[tokio::test]
	async fn test_kubectl_apply_writes_valid_yaml() {
		// Arrange
		let spec = build_project_spec(None, "test-app", "test:v1".to_string(), 2, None)
			.expect("spec should build");
		let crd = build_project_crd(
			"test-app",
			"staging",
			&spec,
			"paas.reinhardt-cloud.dev/v1alpha2",
		);
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
		let spec = build_project_spec(None, "ctx-app", "ctx:v1".to_string(), 1, None)
			.expect("spec should build");
		let crd = build_project_crd(
			"ctx-app",
			"prod",
			&spec,
			"paas.reinhardt-cloud.dev/v1alpha2",
		);
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

	#[rstest]
	#[case("paas.reinhardt-cloud.dev/v1")]
	#[case("paas.reinhardt-cloud.dev/v1alpha2")]
	fn test_validate_api_version_accepts_fully_qualified(#[case] value: &str) {
		// Act
		let result = validate_api_version(value);

		// Assert
		assert_eq!(result.as_deref(), Ok(value));
	}

	#[rstest]
	#[case("v1")]
	#[case("/v1")]
	#[case("paas.reinhardt-cloud.dev/")]
	#[case("paas.reinhardt-cloud.dev/v1/extra")]
	#[case("")]
	// Whitespace-only segments must also be rejected.
	#[case("  /v1")]
	#[case("paas.reinhardt-cloud.dev/  ")]
	#[case("  /  ")]
	// Internal whitespace within a segment is also invalid.
	#[case("paas reinhardt-cloud.dev/v1")]
	#[case("paas.reinhardt-cloud.dev/v 1")]
	fn test_validate_api_version_rejects_malformed(#[case] value: &str) {
		// Act
		let result = validate_api_version(value);

		// Assert
		assert!(
			result.is_err(),
			"expected `{value}` to be rejected as malformed, got Ok"
		);
	}

	#[tokio::test]
	async fn test_resolve_deploy_api_version_non_direct_uses_override() {
		// Arrange / Act
		let resolved = resolve_deploy_api_version(false, Some("paas.reinhardt-cloud.dev/v9"), None)
			.await
			.expect("non-direct override path must not contact a cluster");

		// Assert
		assert_eq!(resolved, "paas.reinhardt-cloud.dev/v9");
	}

	#[tokio::test]
	async fn test_resolve_deploy_api_version_non_direct_falls_back_to_compile_default() {
		// Arrange / Act
		let resolved = resolve_deploy_api_version(false, None, None)
			.await
			.expect("non-direct path with no override must use compile-time default");

		// Assert
		assert_eq!(resolved, COMPILE_TIME_DEFAULT);
	}

	#[tokio::test]
	async fn test_resolve_deploy_api_version_direct_with_override_short_circuits() {
		// Arrange: pass a cluster context that almost certainly does not
		// resolve in the test environment. If the override path is honored,
		// no kubeconfig lookup happens and the override is returned verbatim.
		let resolved = resolve_deploy_api_version(
			true,
			Some("paas.reinhardt-cloud.dev/v9"),
			Some("nonexistent-context-for-testing"),
		)
		.await
		.expect("direct + explicit override must short-circuit cluster discovery");

		// Assert
		assert_eq!(resolved, "paas.reinhardt-cloud.dev/v9");
	}
}
