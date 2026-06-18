use std::collections::BTreeMap;

use anyhow::Result;
use chrono::{Duration, Utc};
use k8s_openapi::api::core::v1::LocalObjectReference;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference;
use kube::ResourceExt;
use kube::api::{Api, DeleteParams, ObjectMeta, PostParams};
use reinhardt_cloud_types::crd::policy::DeletionPolicy;
use reinhardt_cloud_types::crd::source::{
	BuildSpec, GitProvider, PreviewBudget, PreviewOverrides, PreviewSpec, SourceSpec,
};
use reinhardt_cloud_types::crd::{Project, ProjectSpec, ServicesSpec};
use rstest::rstest;
use serial_test::serial;

use crate::harness::{E2eHarness, e2e_labels};

const PREVIEW_PARENT: &str = "preview-parent";
const PREVIEW_NAME: &str = "preview-parent-pr-42";
const EXPIRED_PREVIEW_NAME: &str = "preview-parent-pr-99";
/// Dedicated preview namespace the operator provisions for `PREVIEW_PARENT`
/// (`{parent}-preview`). Preview child Projects live here (#707).
const PREVIEW_NAMESPACE: &str = "preview-parent-preview";

#[rstest]
#[tokio::test]
#[serial(source_pipeline_e2e)]
async fn preview_lifecycle_ttl_and_owner_cascade_are_reconciled() -> Result<()> {
	let Some(harness) = E2eHarness::start("preview").await? else {
		return Ok(());
	};

	let parent = preview_parent_project(
		harness.namespace(),
		Some(preview_annotations(
			"create",
			"42",
			"feature/login",
			"abcdef1234567890",
		)),
	);
	harness.create_project(&parent).await?;

	let created = harness
		.wait_project_in(
			PREVIEW_NAMESPACE,
			PREVIEW_NAME,
			"preview creation",
			|project| {
				project.spec.image == "registry.local/reinhardt/preview-parent:pr-42-abcdef12"
			},
		)
		.await?;
	assert_preview_project(
		&created,
		"feature-login.pr-42.preview-parent-pr-42.example.test",
	);

	harness
		.wait_project(PREVIEW_PARENT, "preview annotations cleared", |project| {
			!project
				.metadata
				.annotations
				.as_ref()
				.is_some_and(|annotations| {
					annotations.contains_key("reinhardt.dev/preview-action")
						|| annotations.contains_key("reinhardt.dev/pr-number")
						|| annotations.contains_key("reinhardt.dev/pr-branch")
				})
		})
		.await?;

	harness
		.patch_project(
			PREVIEW_PARENT,
			serde_json::json!({
				"metadata": {
					"annotations": preview_annotations(
						"update",
						"42",
						"feature/updated",
						"fedcba9876543210",
					)
				}
			}),
		)
		.await?;
	let updated = harness
		.wait_project_in(
			PREVIEW_NAMESPACE,
			PREVIEW_NAME,
			"preview update",
			|project| {
				project.spec.image == "registry.local/reinhardt/preview-parent:pr-42-fedcba98"
					&& project
						.spec
						.services
						.as_ref()
						.and_then(|services| services.ingress_host.as_deref())
						== Some("feature-updated.pr-42.preview-parent-pr-42.example.test")
			},
		)
		.await?;
	assert_preview_project(
		&updated,
		"feature-updated.pr-42.preview-parent-pr-42.example.test",
	);

	harness
		.patch_project(
			PREVIEW_PARENT,
			serde_json::json!({
				"metadata": {
					"annotations": {
						"reinhardt.dev/preview-action": "delete",
						"reinhardt.dev/pr-number": "42"
					}
				}
			}),
		)
		.await?;
	harness
		.wait_project_absent_in(PREVIEW_NAMESPACE, PREVIEW_NAME)
		.await?;

	let live_parent = harness
		.wait_project(PREVIEW_PARENT, "parent uid available", |project| {
			project.metadata.uid.is_some()
		})
		.await?;
	// Seed an already-expired preview directly in the dedicated preview
	// namespace so the TTL cleanup pass (which lists previews there) finds it.
	let expired_preview = expired_preview_project(PREVIEW_NAMESPACE, &live_parent);
	let preview_ns_api: Api<Project> = Api::namespaced(harness.client().clone(), PREVIEW_NAMESPACE);
	preview_ns_api
		.create(&PostParams::default(), &expired_preview)
		.await?;
	harness
		.patch_project(
			PREVIEW_PARENT,
			serde_json::json!({
				"metadata": {
					"annotations": {
						"reinhardt.dev/e2e-ttl-trigger": Utc::now().to_rfc3339()
					}
				}
			}),
		)
		.await?;
	harness
		.wait_project_absent_in(PREVIEW_NAMESPACE, EXPIRED_PREVIEW_NAME)
		.await?;

	harness
		.patch_project(
			PREVIEW_PARENT,
			serde_json::json!({
				"metadata": {
					"annotations": preview_annotations(
						"create",
						"42",
						"feature/cascade",
						"1234567890abcdef",
					)
				}
			}),
		)
		.await?;
	harness
		.wait_project_in(
			PREVIEW_NAMESPACE,
			PREVIEW_NAME,
			"preview recreation before parent delete",
			|project| {
				project.spec.image == "registry.local/reinhardt/preview-parent:pr-42-12345678"
			},
		)
		.await?;
	harness
		.projects()
		.delete(PREVIEW_PARENT, &DeleteParams::default())
		.await?;
	harness.wait_project_absent(PREVIEW_PARENT).await?;
	// Parent deletion cascade-removes the dedicated preview namespace and the
	// preview child Project inside it (#707).
	harness
		.wait_project_absent_in(PREVIEW_NAMESPACE, PREVIEW_NAME)
		.await?;
	harness.wait_namespace_absent(PREVIEW_NAMESPACE).await?;

	harness.collect_diagnostics("preview").await;
	Ok(())
}

fn assert_preview_project(project: &Project, expected_host: &str) {
	assert_eq!(project.name_any(), PREVIEW_NAME);
	assert_eq!(
		project
			.metadata
			.labels
			.as_ref()
			.and_then(|labels| labels.get("reinhardt.dev/preview").map(String::as_str)),
		Some("true")
	);
	assert_eq!(
		project
			.metadata
			.labels
			.as_ref()
			.and_then(|labels| labels.get("reinhardt.dev/parent-app").map(String::as_str)),
		Some(PREVIEW_PARENT)
	);
	assert_eq!(
		project
			.metadata
			.labels
			.as_ref()
			.and_then(|labels| labels.get("reinhardt.dev/pr-number").map(String::as_str)),
		Some("42")
	);
	assert_eq!(
		project
			.metadata
			.owner_references
			.as_ref()
			.and_then(|refs| refs.first())
			.map(|owner| owner.name.as_str()),
		Some(PREVIEW_PARENT)
	);
	assert_eq!(project.spec.replicas, Some(1));
	assert_eq!(project.spec.deletion_policy, DeletionPolicy::Delete);
	assert_eq!(project.spec.source, None);
	assert_eq!(
		project.spec.env.get("REINHARDT_ENV").map(String::as_str),
		Some("preview")
	);
	assert_eq!(
		project
			.spec
			.image_pull_secrets
			.as_ref()
			.and_then(|refs| refs.first())
			.map(|reference| reference.name.as_str()),
		Some("registry-pull-secret")
	);
	assert_eq!(
		project
			.spec
			.services
			.as_ref()
			.and_then(|services| services.ingress_host.as_deref()),
		Some(expected_host)
	);
}

fn preview_parent_project(
	namespace: &str,
	annotations: Option<BTreeMap<String, String>>,
) -> Project {
	Project {
		metadata: ObjectMeta {
			name: Some(PREVIEW_PARENT.to_string()),
			namespace: Some(namespace.to_string()),
			labels: Some(e2e_labels()),
			annotations,
			..Default::default()
		},
		spec: ProjectSpec {
			image: "registry.local/reinhardt/preview-parent:placeholder".to_string(),
			replicas: Some(0),
			services: Some(ServicesSpec {
				port: Some(80),
				target_port: Some(8080),
				ingress_host: Some("preview-parent.example.test".to_string()),
				tls: None,
			}),
			env: BTreeMap::from([("REINHARDT_ENV".to_string(), "preview".to_string())]),
			source: Some(SourceSpec {
				repository: "https://github.com/kent8192/reinhardt-cloud".to_string(),
				branch: Some("main".to_string()),
				provider: Some(GitProvider::GitHub),
				credentials_secret: None,
				build: Some(BuildSpec {
					dockerfile: Some("./Dockerfile".to_string()),
					context: Some(".".to_string()),
					registry: Some("registry.local/reinhardt/preview-parent".to_string()),
					build_args: BTreeMap::new(),
				}),
				webhook: None,
				preview: Some(PreviewSpec {
					enabled: true,
					ttl: Some("1h".to_string()),
					url_template: Some("{branch}.pr-{pr_number}.{app}.example.test".to_string()),
					overrides: Some(PreviewOverrides {
						replicas: Some(1),
						database: Some(false),
						cache: Some(false),
					}),
					budget: Some(PreviewBudget {
						max_replicas: Some(2),
						max_cpu: Some("2".to_string()),
						max_memory: Some("4Gi".to_string()),
					}),
				}),
			}),
			image_pull_secrets: Some(vec![LocalObjectReference {
				name: "registry-pull-secret".to_string(),
			}]),
			..Default::default()
		},
		status: None,
	}
}

fn expired_preview_project(namespace: &str, parent: &Project) -> Project {
	let owner = OwnerReference {
		api_version: "paas.reinhardt-cloud.dev/v1alpha2".to_string(),
		kind: "Project".to_string(),
		name: PREVIEW_PARENT.to_string(),
		uid: parent
			.metadata
			.uid
			.clone()
			.expect("parent uid should be available"),
		controller: Some(true),
		block_owner_deletion: Some(true),
	};
	Project {
		metadata: ObjectMeta {
			name: Some(EXPIRED_PREVIEW_NAME.to_string()),
			namespace: Some(namespace.to_string()),
			labels: Some(BTreeMap::from([
				("reinhardt.dev/preview".to_string(), "true".to_string()),
				(
					"reinhardt.dev/parent-app".to_string(),
					PREVIEW_PARENT.to_string(),
				),
				("reinhardt.dev/pr-number".to_string(), "99".to_string()),
				(
					"app.kubernetes.io/managed-by".to_string(),
					"reinhardt-cloud".to_string(),
				),
			])),
			annotations: Some(BTreeMap::from([(
				"reinhardt.dev/last-activity".to_string(),
				(Utc::now() - Duration::hours(2)).to_rfc3339(),
			)])),
			owner_references: Some(vec![owner]),
			..Default::default()
		},
		spec: ProjectSpec {
			image: "registry.local/reinhardt/preview-parent:pr-99-expired".to_string(),
			replicas: Some(0),
			deletion_policy: DeletionPolicy::Delete,
			..Default::default()
		},
		status: None,
	}
}

fn preview_annotations(
	action: &str,
	pr_number: &str,
	branch: &str,
	commit_sha: &str,
) -> BTreeMap<String, String> {
	BTreeMap::from([
		(
			"reinhardt.dev/preview-action".to_string(),
			action.to_string(),
		),
		("reinhardt.dev/pr-number".to_string(), pr_number.to_string()),
		("reinhardt.dev/pr-branch".to_string(), branch.to_string()),
		(
			"reinhardt.dev/build-trigger".to_string(),
			commit_sha.to_string(),
		),
	])
}
