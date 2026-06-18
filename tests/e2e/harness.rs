use std::fs::{File, create_dir_all};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use k8s_openapi::api::batch::v1::Job;
use k8s_openapi::api::core::v1::Namespace;
use kube::api::{Api, ListParams, ObjectMeta, Patch, PatchParams, PostParams};
use kube::{Client, ResourceExt};
use reinhardt_cloud_types::crd::Project;
use uuid::Uuid;

pub(crate) const RUN_ENV: &str = "REINHARDT_CLOUD_SOURCE_PIPELINE_E2E";
pub(crate) const SUITE_LABEL_KEY: &str = "reinhardt.dev/e2e-suite";
pub(crate) const SUITE_LABEL_VALUE: &str = "source-pipeline";

const KEEP_ENV: &str = "REINHARDT_CLOUD_SOURCE_PIPELINE_E2E_KEEP_RESOURCES";
const NAMESPACE_ENV: &str = "REINHARDT_CLOUD_SOURCE_PIPELINE_E2E_NAMESPACE";
const TIMEOUT_ENV: &str = "REINHARDT_CLOUD_SOURCE_PIPELINE_E2E_TIMEOUT_SECONDS";
const OPERATOR_MODE_ENV: &str = "REINHARDT_CLOUD_E2E_OPERATOR_MODE";
const OPERATOR_BIN_ENV: &str = "REINHARDT_CLOUD_E2E_OPERATOR_BIN";
const ARTIFACT_DIR_ENV: &str = "REINHARDT_CLOUD_SOURCE_PIPELINE_E2E_ARTIFACT_DIR";

pub(crate) struct E2eHarness {
	client: Client,
	namespace: String,
	created_namespace: bool,
	keep_resources: bool,
	operator: Option<Child>,
	artifact_dir: PathBuf,
	timeout: Duration,
}

impl E2eHarness {
	pub(crate) async fn start(test_name: &str) -> Result<Option<Self>> {
		if std::env::var(RUN_ENV).ok().as_deref() != Some("1") {
			eprintln!("skipping source pipeline E2E; set {RUN_ENV}=1 to run it");
			return Ok(None);
		}

		require_command("kubectl")?;
		run_kubectl(&["version", "--client"])?;
		run_kubectl(&["cluster-info"])?;
		ensure_crd()?;

		let client = Client::try_default()
			.await
			.context("failed to create Kubernetes client from the active context")?;
		let timeout = timeout();
		let namespace = namespace_name(test_name);
		let artifact_dir = artifact_dir(&namespace);
		create_dir_all(&artifact_dir).context("failed to create E2E artifact directory")?;
		let created_namespace = ensure_namespace(&client, &namespace).await?;
		let keep_resources = std::env::var(KEEP_ENV).ok().as_deref() == Some("1");
		let operator = ensure_operator(&artifact_dir)?;

		Ok(Some(Self {
			client,
			namespace,
			created_namespace,
			keep_resources,
			operator,
			artifact_dir,
			timeout,
		}))
	}

	pub(crate) fn namespace(&self) -> &str {
		&self.namespace
	}

	pub(crate) fn projects(&self) -> Api<Project> {
		Api::namespaced(self.client.clone(), &self.namespace)
	}

	pub(crate) fn jobs(&self) -> Api<Job> {
		Api::namespaced(self.client.clone(), &self.namespace)
	}

	pub(crate) async fn create_project(&self, project: &Project) -> Result<Project> {
		self.projects()
			.create(&PostParams::default(), project)
			.await
			.with_context(|| format!("failed to create Project {}", project.name_any()))
	}

	pub(crate) async fn patch_project(
		&self,
		name: &str,
		patch: serde_json::Value,
	) -> Result<Project> {
		self.projects()
			.patch(name, &PatchParams::default(), &Patch::Merge(&patch))
			.await
			.with_context(|| format!("failed to patch Project {name}"))
	}

	pub(crate) async fn wait_project<F>(
		&self,
		name: &str,
		description: &str,
		predicate: F,
	) -> Result<Project>
	where
		F: Fn(&Project) -> bool,
	{
		let deadline = Instant::now() + self.timeout;
		let projects = self.projects();
		let mut last: Option<Project> = None;

		while Instant::now() < deadline {
			if let Some(project) = projects
				.get_opt(name)
				.await
				.with_context(|| format!("failed to get Project {name}"))?
			{
				if predicate(&project) {
					return Ok(project);
				}
				last = Some(project);
			}
			tokio::time::sleep(Duration::from_millis(500)).await;
		}

		bail!("Project {name} did not satisfy {description}; last state: {last:#?}")
	}

	pub(crate) async fn wait_project_absent(&self, name: &str) -> Result<()> {
		let deadline = Instant::now() + self.timeout;
		let projects = self.projects();

		while Instant::now() < deadline {
			if projects
				.get_opt(name)
				.await
				.with_context(|| format!("failed to get Project {name}"))?
				.is_none()
			{
				return Ok(());
			}
			tokio::time::sleep(Duration::from_millis(500)).await;
		}

		bail!("Project {name} still existed after {:?}", self.timeout)
	}

	/// Raw kube client, for ad-hoc cross-namespace operations (e.g. preview
	/// child Projects that live in a `{parent}-preview` namespace distinct
	/// from the test namespace).
	pub(crate) fn client(&self) -> &Client {
		&self.client
	}

	/// Like [`wait_project`](Self::wait_project) but in an explicit namespace.
	pub(crate) async fn wait_project_in<F>(
		&self,
		namespace: &str,
		name: &str,
		description: &str,
		predicate: F,
	) -> Result<Project>
	where
		F: Fn(&Project) -> bool,
	{
		let deadline = Instant::now() + self.timeout;
		let projects: Api<Project> = Api::namespaced(self.client.clone(), namespace);
		let mut last: Option<Project> = None;
		while Instant::now() < deadline {
			if let Some(project) = projects
				.get_opt(name)
				.await
				.with_context(|| format!("failed to get Project {namespace}/{name}"))?
			{
				if predicate(&project) {
					return Ok(project);
				}
				last = Some(project);
			}
			tokio::time::sleep(Duration::from_millis(500)).await;
		}
		bail!("Project {namespace}/{name} did not satisfy {description}; last state: {last:#?}")
	}

	/// Like [`wait_project_absent`](Self::wait_project_absent) but in an
	/// explicit namespace.
	pub(crate) async fn wait_project_absent_in(&self, namespace: &str, name: &str) -> Result<()> {
		let deadline = Instant::now() + self.timeout;
		let projects: Api<Project> = Api::namespaced(self.client.clone(), namespace);
		while Instant::now() < deadline {
			if projects
				.get_opt(name)
				.await
				.with_context(|| format!("failed to get Project {namespace}/{name}"))?
				.is_none()
			{
				return Ok(());
			}
			tokio::time::sleep(Duration::from_millis(500)).await;
		}
		bail!(
			"Project {namespace}/{name} still existed after {:?}",
			self.timeout
		)
	}

	/// Waits for a namespace to be absent (used to assert preview-namespace
	/// cascade cleanup when a parent Project is deleted).
	pub(crate) async fn wait_namespace_absent(&self, namespace: &str) -> Result<()> {
		let deadline = Instant::now() + self.timeout;
		let namespaces: Api<k8s_openapi::api::core::v1::Namespace> = Api::all(self.client.clone());
		while Instant::now() < deadline {
			if namespaces
				.get_opt(namespace)
				.await
				.with_context(|| format!("failed to get Namespace {namespace}"))?
				.is_none()
			{
				return Ok(());
			}
			tokio::time::sleep(Duration::from_millis(500)).await;
		}
		bail!(
			"Namespace {namespace} still existed after {:?}",
			self.timeout
		)
	}

	pub(crate) async fn wait_job_named(&self, name: &str) -> Result<Job> {
		let deadline = Instant::now() + self.timeout;
		let jobs = self.jobs();

		while Instant::now() < deadline {
			if let Some(job) = jobs
				.get_opt(name)
				.await
				.with_context(|| format!("failed to get Job {name}"))?
			{
				return Ok(job);
			}
			tokio::time::sleep(Duration::from_millis(500)).await;
		}

		bail!("Job {name} was not created within {:?}", self.timeout)
	}

	pub(crate) async fn assert_no_job_named_after(
		&self,
		name: &str,
		delay: Duration,
	) -> Result<()> {
		tokio::time::sleep(delay).await;
		let existing = self
			.jobs()
			.get_opt(name)
			.await
			.with_context(|| format!("failed to check Job {name}"))?;
		if existing.is_some() {
			bail!("Job {name} should not have been created");
		}
		Ok(())
	}

	pub(crate) async fn collect_diagnostics(&self, test_name: &str) {
		let safe_test = sanitize_label(test_name);
		let _ = kubectl_to_file(
			&[
				"get",
				"project,job,deployment,service,pod",
				"-n",
				&self.namespace,
				"-o",
				"yaml",
			],
			&self
				.artifact_dir
				.join(format!("{safe_test}-resources.yaml")),
		);
		let _ = kubectl_to_file(
			&[
				"get",
				"events",
				"-n",
				&self.namespace,
				"--sort-by=.lastTimestamp",
			],
			&self.artifact_dir.join(format!("{safe_test}-events.txt")),
		);
	}
}

impl Drop for E2eHarness {
	fn drop(&mut self) {
		let _ = self.collect_drop_diagnostics();

		if let Some(mut child) = self.operator.take() {
			stop_process(&mut child);
		}

		if self.keep_resources {
			eprintln!(
				"keeping source pipeline E2E resources in namespace {} and artifacts {}",
				self.namespace,
				self.artifact_dir.display()
			);
			return;
		}

		if self.created_namespace {
			let _ = run_kubectl(&[
				"delete",
				"namespace",
				&self.namespace,
				"--ignore-not-found",
				"--wait=false",
			]);
		} else {
			let selector = format!("{SUITE_LABEL_KEY}={SUITE_LABEL_VALUE}");
			let _ = run_kubectl(&[
				"delete",
				"project",
				"-n",
				&self.namespace,
				"-l",
				&selector,
				"--ignore-not-found",
			]);
		}
	}
}

impl E2eHarness {
	fn collect_drop_diagnostics(&self) -> Result<()> {
		kubectl_to_file(
			&[
				"get",
				"project,job,deployment,service,pod",
				"-n",
				&self.namespace,
				"-o",
				"yaml",
			],
			&self.artifact_dir.join("drop-resources.yaml"),
		)?;
		kubectl_to_file(
			&[
				"get",
				"events",
				"-n",
				&self.namespace,
				"--sort-by=.lastTimestamp",
			],
			&self.artifact_dir.join("drop-events.txt"),
		)?;
		Ok(())
	}
}

pub(crate) fn e2e_labels() -> std::collections::BTreeMap<String, String> {
	std::collections::BTreeMap::from([(SUITE_LABEL_KEY.to_string(), SUITE_LABEL_VALUE.to_string())])
}

async fn ensure_namespace(client: &Client, namespace: &str) -> Result<bool> {
	let namespaces: Api<Namespace> = Api::all(client.clone());
	if namespaces
		.get_opt(namespace)
		.await
		.with_context(|| format!("failed to check Namespace {namespace}"))?
		.is_some()
	{
		return Ok(false);
	}

	namespaces
		.create(
			&PostParams::default(),
			&Namespace {
				metadata: ObjectMeta {
					name: Some(namespace.to_string()),
					labels: Some(e2e_labels()),
					..Default::default()
				},
				..Default::default()
			},
		)
		.await
		.with_context(|| format!("failed to create Namespace {namespace}"))?;
	Ok(true)
}

fn ensure_crd() -> Result<()> {
	let root = workspace_root();
	let crd_path = root.join("charts/reinhardt-cloud-operator/crds/project-crd.yaml");
	let crd = crd_path
		.to_str()
		.context("Project CRD path is not valid UTF-8")?;
	run_kubectl(&["apply", "-f", crd])?;
	run_kubectl(&[
		"wait",
		"--for=condition=Established",
		"crd/projects.paas.reinhardt-cloud.dev",
		"--timeout=120s",
	])
}

fn ensure_operator(artifact_dir: &Path) -> Result<Option<Child>> {
	let mode = std::env::var(OPERATOR_MODE_ENV).unwrap_or_else(|_| "auto".to_string());
	match mode.as_str() {
		"existing" => {
			if !operator_deployment_exists()? {
				bail!("no in-cluster reinhardt-cloud operator Deployment was found");
			}
			Ok(None)
		}
		"skip" => Ok(None),
		"local" => start_local_operator(artifact_dir).map(Some),
		"auto" => {
			if operator_deployment_exists()? {
				Ok(None)
			} else {
				start_local_operator(artifact_dir).map(Some)
			}
		}
		other => {
			bail!("invalid {OPERATOR_MODE_ENV}={other}; expected auto, existing, local, or skip")
		}
	}
}

fn operator_deployment_exists() -> Result<bool> {
	let output = Command::new("kubectl")
		.args([
			"get",
			"deployment",
			"-A",
			"-l",
			"app.kubernetes.io/name=reinhardt-cloud-operator",
			"-o",
			"name",
		])
		.output()
		.context("failed to query in-cluster operator Deployment")?;
	if !output.status.success() {
		return Ok(false);
	}
	Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

fn start_local_operator(artifact_dir: &Path) -> Result<Child> {
	let mut operator_bin = std::env::var(OPERATOR_BIN_ENV)
		.map(PathBuf::from)
		.unwrap_or_else(|_| workspace_root().join("target/debug/reinhardt-cloud-operator"));
	if operator_bin.is_relative() {
		operator_bin = workspace_root().join(operator_bin);
	}
	if !operator_bin.exists() {
		bail!(
			"operator binary {} does not exist; build it first or set {OPERATOR_BIN_ENV}",
			operator_bin.display()
		);
	}

	let log_path = artifact_dir.join("operator.log");
	let log = File::create(&log_path)
		.with_context(|| format!("failed to create operator log {}", log_path.display()))?;
	let metrics_addr = random_local_addr()?;
	let mut child = Command::new(&operator_bin)
		.current_dir(workspace_root())
		.env("REINHARDT_CLOUD_METRICS_ADDR", metrics_addr)
		.env("REINHARDT_CLOUD_METRICS_ENABLED", "0")
		.stdout(Stdio::from(
			log.try_clone().context("failed to clone operator log")?,
		))
		.stderr(Stdio::from(log))
		.spawn()
		.with_context(|| format!("failed to start operator {}", operator_bin.display()))?;

	std::thread::sleep(Duration::from_secs(3));
	if let Some(status) = child
		.try_wait()
		.context("failed to inspect local operator status")?
	{
		bail!(
			"local operator exited with {status}; see {}",
			log_path.display()
		);
	}

	Ok(child)
}

fn stop_process(child: &mut Child) {
	let _ = child.kill();
	let _ = child.wait();
}

fn run_kubectl(args: &[&str]) -> Result<()> {
	let output = Command::new("kubectl")
		.args(args)
		.output()
		.with_context(|| format!("failed to run kubectl {}", args.join(" ")))?;
	if !output.status.success() {
		bail!(
			"kubectl {} failed with status {}\nstdout:\n{}\nstderr:\n{}",
			args.join(" "),
			output.status,
			String::from_utf8_lossy(&output.stdout),
			String::from_utf8_lossy(&output.stderr)
		);
	}
	Ok(())
}

fn kubectl_to_file(args: &[&str], path: &Path) -> Result<()> {
	let output = Command::new("kubectl")
		.args(args)
		.output()
		.with_context(|| format!("failed to run kubectl {}", args.join(" ")))?;
	std::fs::write(path, [&output.stdout[..], &output.stderr[..]].concat())
		.with_context(|| format!("failed to write {}", path.display()))?;
	Ok(())
}

fn require_command(command: &str) -> Result<()> {
	let status = Command::new(command)
		.arg("--help")
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.status()
		.with_context(|| format!("{command} is required for source pipeline E2E"))?;
	if !status.success() {
		bail!("{command} is required for source pipeline E2E");
	}
	Ok(())
}

fn namespace_name(test_name: &str) -> String {
	if let Ok(namespace) = std::env::var(NAMESPACE_ENV)
		&& !namespace.trim().is_empty()
	{
		return namespace;
	}
	let suffix = Uuid::now_v7().to_string();
	format!("rc-e2e-{}-{}", sanitize_label(test_name), &suffix[..8])
}

fn artifact_dir(namespace: &str) -> PathBuf {
	std::env::var(ARTIFACT_DIR_ENV)
		.map(PathBuf::from)
		.unwrap_or_else(|_| {
			workspace_root()
				.join("target/source-pipeline-e2e")
				.join(namespace)
		})
}

fn timeout() -> Duration {
	std::env::var(TIMEOUT_ENV)
		.ok()
		.and_then(|value| value.parse::<u64>().ok())
		.map(Duration::from_secs)
		.unwrap_or_else(|| Duration::from_secs(90))
}

fn workspace_root() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR"))
		.parent()
		.expect("tests crate should be inside the workspace root")
		.to_path_buf()
}

fn random_local_addr() -> Result<String> {
	let listener = std::net::TcpListener::bind("127.0.0.1:0")
		.context("failed to reserve a local operator metrics address")?;
	let addr = listener
		.local_addr()
		.context("failed to read local metrics address")?;
	Ok(addr.to_string())
}

fn sanitize_label(value: &str) -> String {
	let mut label = String::with_capacity(value.len());
	let mut previous_dash = false;
	for character in value.chars().flat_map(char::to_lowercase) {
		let normalized = if character.is_ascii_alphanumeric() {
			character
		} else {
			'-'
		};
		let character = normalized;
		if character == '-' {
			if label.is_empty() || previous_dash {
				continue;
			}
			previous_dash = true;
		} else {
			previous_dash = false;
		}
		if label.len() == 48 {
			break;
		}
		label.push(character);
	}
	while label.ends_with('-') {
		label.pop();
	}
	if label.is_empty() {
		"test".to_string()
	} else {
		label
	}
}

pub(crate) async fn list_jobs(api: &Api<Job>) -> Result<Vec<Job>> {
	api.list(&ListParams::default())
		.await
		.context("failed to list Jobs")
		.map(|list| list.items)
}
