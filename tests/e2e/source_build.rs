use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::Result;
use k8s_openapi::api::core::v1::LocalObjectReference;
use kube::ResourceExt;
use kube::api::ObjectMeta;
use reinhardt_cloud_types::crd::source::{BuildSpec, GitProvider, SourceSpec};
use reinhardt_cloud_types::crd::{Project, ProjectSpec};
use rstest::rstest;
use serial_test::serial;

use crate::harness::{E2eHarness, e2e_labels, list_jobs};

const SOURCE_APP: &str = "source-build-app";
const IMAGE_ONLY_APP: &str = "image-only-app";

#[rstest]
#[tokio::test]
#[serial(source_pipeline_e2e)]
async fn source_build_trigger_creates_kaniko_job_and_updates_parent_image() -> Result<()> {
	let Some(harness) = E2eHarness::start("source-build").await? else {
		return Ok(());
	};

	let source_project =
		source_build_project(harness.namespace(), SOURCE_APP, Some("abcdef1234567890"));
	harness.create_project(&source_project).await?;

	let job = harness
		.wait_job_named("source-build-app-build-abcdef12")
		.await?;
	let job_metadata = job.metadata;
	assert_eq!(
		job_metadata.labels.as_ref().and_then(|labels| {
			labels
				.get("app.kubernetes.io/component")
				.map(String::as_str)
		}),
		Some("build")
	);
	assert_eq!(
		job_metadata
			.owner_references
			.as_ref()
			.and_then(|refs| refs.first())
			.map(|owner| owner.name.as_str()),
		Some(SOURCE_APP)
	);

	let pod_spec = job
		.spec
		.as_ref()
		.and_then(|spec| spec.template.spec.as_ref())
		.expect("Kaniko Job should include a Pod spec");
	assert_eq!(pod_spec.restart_policy.as_deref(), Some("Never"));
	assert_eq!(
		pod_spec
			.image_pull_secrets
			.as_ref()
			.and_then(|refs| refs.first())
			.map(|reference| reference.name.as_str()),
		Some("registry-pull-secret")
	);
	let container = pod_spec
		.containers
		.first()
		.expect("Kaniko Job should include one container");
	assert_eq!(container.name, "kaniko");
	assert_eq!(
		container.image.as_deref(),
		Some("gcr.io/kaniko-project/executor:latest")
	);
	assert_eq!(
		container.args.as_deref(),
		Some(
			[
				"--git=branch=main,url=https://github.com/kent8192/reinhardt-cloud".to_string(),
				"--dockerfile=./Dockerfile.e2e".to_string(),
				"--context=dir://.".to_string(),
				"--destination=registry.local/reinhardt/source-build-app:source-build-app-abcdef12"
					.to_string(),
				"--cache=true".to_string(),
				"--build-arg=PROFILE=e2e".to_string(),
			]
			.as_slice()
		)
	);
	assert_eq!(
		container.env.as_ref().map(|env| env
			.iter()
			.map(|item| item.name.as_str())
			.collect::<Vec<_>>()),
		Some(vec!["GIT_USERNAME", "GIT_PASSWORD"])
	);
	assert_eq!(
		container
			.volume_mounts
			.as_ref()
			.and_then(|mounts| mounts.first())
			.map(|mount| (mount.name.as_str(), mount.mount_path.as_str())),
		Some(("registry-auth", "/kaniko/.docker/config.json"))
	);
	assert_eq!(
		pod_spec
			.volumes
			.as_ref()
			.and_then(|volumes| volumes.first())
			.map(|volume| volume.name.as_str()),
		Some("registry-auth")
	);

	harness
		.wait_project(SOURCE_APP, "built image patch", |project| {
			project.spec.image
				== "registry.local/reinhardt/source-build-app:source-build-app-abcdef12"
				&& !project
					.metadata
					.annotations
					.as_ref()
					.is_some_and(|annotations| {
						annotations.contains_key("reinhardt.dev/build-trigger")
					})
		})
		.await?;

	let image_only_project = image_only_project(harness.namespace(), IMAGE_ONLY_APP);
	harness.create_project(&image_only_project).await?;
	harness
		.assert_no_job_named_after("image-only-app-build-image-only", Duration::from_secs(5))
		.await?;
	let jobs = list_jobs(&harness.jobs()).await?;
	let image_only_jobs = jobs
		.iter()
		.filter(|job| job.name_any().starts_with("image-only-app-build-"))
		.count();
	assert_eq!(image_only_jobs, 0);

	harness.collect_diagnostics("source-build").await;
	Ok(())
}

fn source_build_project(namespace: &str, name: &str, trigger: Option<&str>) -> Project {
	let annotations = trigger.map(|value| {
		BTreeMap::from([("reinhardt.dev/build-trigger".to_string(), value.to_string())])
	});

	Project {
		metadata: ObjectMeta {
			name: Some(name.to_string()),
			namespace: Some(namespace.to_string()),
			labels: Some(e2e_labels()),
			annotations,
			..Default::default()
		},
		spec: ProjectSpec {
			image: "registry.local/reinhardt/source-build-app:placeholder".to_string(),
			replicas: Some(0),
			source: Some(SourceSpec {
				repository: "https://github.com/kent8192/reinhardt-cloud".to_string(),
				branch: Some("main".to_string()),
				provider: Some(GitProvider::GitHub),
				credentials_secret: Some("git-credentials".to_string()),
				build: Some(BuildSpec {
					dockerfile: Some("./Dockerfile.e2e".to_string()),
					context: Some(".".to_string()),
					registry: Some("registry.local/reinhardt/source-build-app".to_string()),
					build_args: BTreeMap::from([("PROFILE".to_string(), "e2e".to_string())]),
				}),
				webhook: None,
				preview: None,
			}),
			image_pull_secrets: Some(vec![LocalObjectReference {
				name: "registry-pull-secret".to_string(),
			}]),
			..Default::default()
		},
		status: None,
	}
}

fn image_only_project(namespace: &str, name: &str) -> Project {
	Project {
		metadata: ObjectMeta {
			name: Some(name.to_string()),
			namespace: Some(namespace.to_string()),
			labels: Some(e2e_labels()),
			..Default::default()
		},
		spec: ProjectSpec {
			image: "registry.local/reinhardt/image-only:latest".to_string(),
			replicas: Some(0),
			..Default::default()
		},
		status: None,
	}
}
