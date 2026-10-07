use std::process::Stdio;
use std::time::Duration;

use crate::fixture::CloudFixture;
use cloud_dashboard::apps::cluster::models::{Cluster, EnvironmentAllocation};
use cloud_dashboard::apps::deployment::persistence::{accept_operation, mark_deadline_uncertain};
use cloud_dashboard::apps::deployment::services::{OperationRequest, RuntimeChange};
use cloud_dashboard::apps::organization::models::Membership;
use cloud_dashboard::apps::project::models::{Environment, Project};
use reinhardt::db::orm::{Model, connection::DatabaseConnectionLease};
use rstest::rstest;

#[rstest]
#[tokio::test]
#[ignore = "Requires published Pages assets and a Playwright Node runtime; see dashboard/README.md"]
async fn browser_hydrates_authenticated_project_detail_and_preserves_language() {
	// Arrange
	let node = std::env::var("CLOUD_BROWSER_NODE").expect("Set CLOUD_BROWSER_NODE");
	let packages = std::env::var("CLOUD_BROWSER_PACKAGES").expect("Set CLOUD_BROWSER_PACKAGES");
	let fixture = CloudFixture::new().await;
	fixture.run(&["migrate"]).await;
	let output = fixture
		.command(&["createplatformadmin", "browser-owner@example.test"])
		.env("CLOUD_BOOTSTRAP_PASSWORD", "a-long-browser-password")
		.output()
		.await
		.unwrap();
	assert!(output.status.success());
	let backend = reinhardt::db::backends::DatabaseConnection::connect_postgres_with_pool_size(
		&fixture.database_url(),
		Some(2),
	)
	.await
	.unwrap();
	let lease = DatabaseConnectionLease::register(backend).unwrap();
	let mut connection = lease.handle();
	let member = Membership::objects()
		.all()
		.all_with_db(&mut connection)
		.await
		.unwrap()
		.remove(0);
	let project = Project::new()
		.organization(member.organization_id())
		.display_name("Browser App")
		.repository_url("https://github.com/example/browser-app")
		.finish();
	let project = Project::objects()
		.create_with_conn(&mut connection, &project)
		.await
		.unwrap();
	let environment = Environment::new()
		.organization(member.organization_id())
		.project(project.id)
		.kind("staging")
		.version(0)
		.desired_runtime("{}")
		.finish();
	let environment = Environment::objects()
		.create_with_conn(&mut connection, &environment)
		.await
		.unwrap();
	let cluster = Cluster::new()
		.display_name("Browser Fixture Cluster")
		.registered(true)
		.rootless_builds(true)
		.ingress(true)
		.dns(true)
		.namespace_issuer(true)
		.cpu_autoscaling(true)
		.finish();
	let cluster = Cluster::objects()
		.create_with_conn(&mut connection, &cluster)
		.await
		.unwrap();
	let allocation = EnvironmentAllocation::new()
		.environment(environment.id)
		.cluster(cluster.id)
		.replica_limit(3)
		.finish();
	EnvironmentAllocation::objects()
		.create_with_conn(&mut connection, &allocation)
		.await
		.unwrap();
	let request = OperationRequest {
		organization_id: member.organization_id(),
		environment_id: environment.id,
		expected_version: 0,
		idempotency_key: "browser-scale".into(),
		change: RuntimeChange::Scale { replicas: 2 },
	};
	let operation = accept_operation(&connection, member.user_id(), &request)
		.await
		.unwrap();
	mark_deadline_uncertain(
		&connection,
		cluster.id,
		&operation,
		chrono::Utc::now() + chrono::Duration::minutes(11),
	)
	.await
	.unwrap();
	let address = fixture.origin.strip_prefix("http://").unwrap();
	let mut server = fixture
		.command(&[
			"runserver",
			address,
			"--with-pages",
			"--asset-manifest",
			"dist/manifest.json",
			"--no-spa",
			"--noreload",
			"--no-wasm",
		])
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.spawn()
		.unwrap();
	let client = reqwest::Client::builder()
		.timeout(Duration::from_secs(2))
		.build()
		.unwrap();
	let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
	loop {
		assert!(
			server.try_wait().unwrap().is_none(),
			"Control Plane exited before readiness"
		);
		if client
			.get(format!("{}/healthz/", fixture.origin))
			.send()
			.await
			.is_ok_and(|response| response.status() == reqwest::StatusCode::OK)
		{
			break;
		}
		assert!(
			tokio::time::Instant::now() < deadline,
			"Control Plane readiness deadline exceeded"
		);
		tokio::time::sleep(Duration::from_millis(50)).await;
	}
	// Act
	let artifacts = tempfile::TempDir::new().unwrap();
	let screenshot = artifacts.path().join("dashboard.png");
	let output = tokio::time::timeout(
		Duration::from_secs(120),
		tokio::process::Command::new(node)
			.kill_on_drop(true)
			.arg(
				std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/browser/projects.cjs"),
			)
			.env("CLOUD_BROWSER_PACKAGES", packages)
			.env("CLOUD_BROWSER_URL", format!("{}/", fixture.origin))
			.env("CLOUD_BROWSER_SCREENSHOT", &screenshot)
			.output(),
	)
	.await
	.expect("Browser checks exceeded their deadline")
	.unwrap();
	// Assert
	assert!(
		output.status.success(),
		"{}\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	assert_eq!(
		String::from_utf8(output.stdout).unwrap().trim(),
		"Dashboard browser checks passed"
	);
	if let Some(export) = std::env::var_os("CLOUD_BROWSER_EXPORT_SCREENSHOT") {
		let export = std::path::PathBuf::from(export);
		assert!(
			export.is_absolute(),
			"Screenshot exports require an absolute path"
		);
		std::fs::copy(screenshot, export).unwrap();
	}
}
