use std::time::Duration;

use chrono::Utc;
use k8s_openapi::api::batch::v1::Job;
use kube::api::{DeleteParams, Patch, PatchParams, PostParams};
use kube::{Api, Client, ResourceExt};
use reinhardt_cloud_types::crd::{
	BuildPhase, BuildStatus, BuildTargetKind, ConditionStatus, ConditionType, Project,
	ProjectCondition,
};

use crate::error::Error;
use crate::resources::{preview, source};

pub(crate) const BUILD_TRIGGER_ANNOTATION: &str = "reinhardt.dev/build-trigger";
pub(crate) const PREVIEW_ACTION_ANNOTATION: &str = "reinhardt.dev/preview-action";
pub(crate) const PR_NUMBER_ANNOTATION: &str = "reinhardt.dev/pr-number";
pub(crate) const PR_BRANCH_ANNOTATION: &str = "reinhardt.dev/pr-branch";
pub(crate) const BUILD_REQUEUE_SECS: u64 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BuildDecision {
	NoBuild,
	Waiting { requeue_after: Duration },
	Succeeded(BuildCompletion),
	Failed(BuildFailure),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BuildCompletion {
	pub(crate) status: BuildStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BuildFailure {
	pub(crate) status: BuildStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum JobBuildState {
	Running,
	Succeeded,
	Failed { reason: String, message: String },
}

pub(crate) fn active_build_status(app: &Project) -> Option<BuildStatus> {
	app.status
		.as_ref()
		.and_then(|status| status.build.as_ref())
		.filter(|build| matches!(build.phase, BuildPhase::Pending | BuildPhase::Running))
		.cloned()
}

pub(crate) fn blocking_failed_build_status(
	app: &Project,
	has_new_build: bool,
) -> Option<BuildStatus> {
	if has_new_build {
		return None;
	}

	app.status
		.as_ref()
		.and_then(|status| status.build.as_ref())
		.filter(|build| build.phase == BuildPhase::Failed)
		.cloned()
}

pub(crate) fn derive_new_build_status(app: &Project) -> Result<Option<BuildStatus>, Error> {
	if !source::should_build_from_source(app) {
		return Ok(None);
	}
	let source_spec = app
		.spec
		.source
		.as_ref()
		.ok_or(Error::MissingField("spec.source"))?;
	let Some(trigger) = annotation_value(app, BUILD_TRIGGER_ANNOTATION) else {
		return Ok(None);
	};

	let name = app.name_any();
	let short_trigger: String = trigger.chars().take(8).collect();
	let preview_pr_number = preview_pr_number(app);
	let image_tag = if let Some(pr_number) = preview_pr_number {
		preview::preview_image_tag(pr_number, &short_trigger)
	} else {
		format!("{name}-{short_trigger}")
	};
	let image = source::built_image_reference(app, &image_tag)?;
	let job_name = format!("{name}-build-{}", image_tag_suffix(&image_tag));
	let branch = annotation_value(app, PR_BRANCH_ANNOTATION)
		.filter(|branch| !branch.trim().is_empty())
		.or(source_spec.branch.as_deref())
		.map(str::to_string);
	let now = Utc::now().to_rfc3339();

	Ok(Some(BuildStatus {
		phase: BuildPhase::Pending,
		target: if preview_pr_number.is_some() {
			BuildTargetKind::Preview
		} else {
			BuildTargetKind::Production
		},
		trigger: trigger.to_string(),
		job_name,
		image,
		image_tag,
		preview_name: preview_pr_number
			.map(|pr_number| preview::preview_project_name(&name, pr_number)),
		pr_number: preview_pr_number.map(str::to_string),
		branch,
		reason: Some("BuildPending".to_string()),
		message: Some("Kaniko build Job has been accepted".to_string()),
		started_at: Some(now.clone()),
		last_transition_time: Some(now),
	}))
}

pub(crate) fn classify_job_state(job: &Job) -> JobBuildState {
	let Some(status) = job.status.as_ref() else {
		return JobBuildState::Running;
	};

	if status.succeeded.unwrap_or_default() > 0 {
		return JobBuildState::Succeeded;
	}

	if let Some(condition) = status.conditions.as_ref().and_then(|conditions| {
		conditions
			.iter()
			.find(|condition| condition.type_ == "Failed" && condition.status == "True")
	}) {
		return JobBuildState::Failed {
			reason: condition
				.reason
				.clone()
				.unwrap_or_else(|| "JobFailed".to_string()),
			message: condition
				.message
				.clone()
				.unwrap_or_else(|| "Kaniko build Job reported a failed condition".to_string()),
		};
	}

	let backoff_limit = job
		.spec
		.as_ref()
		.and_then(|spec| spec.backoff_limit)
		.unwrap_or(6);
	if status.failed.unwrap_or_default() > backoff_limit {
		return JobBuildState::Failed {
			reason: "BackoffLimitExceeded".to_string(),
			message: "Kaniko build Job exceeded backoff limit".to_string(),
		};
	}

	JobBuildState::Running
}

pub(crate) fn build_condition(
	type_: ConditionType,
	status: ConditionStatus,
	reason: &str,
	message: &str,
	observed_generation: Option<i64>,
) -> ProjectCondition {
	ProjectCondition {
		type_,
		status,
		reason: reason.to_string(),
		message: message.to_string(),
		last_transition_time: Some(Utc::now().to_rfc3339()),
		observed_generation,
	}
}

fn upsert_condition(conditions: &mut Vec<ProjectCondition>, next: ProjectCondition) {
	if let Some(existing) = conditions
		.iter_mut()
		.find(|condition| condition.type_ == next.type_)
	{
		*existing = next;
	} else {
		conditions.push(next);
	}
}

fn preserve_transition_time_for_unchanged_status(app: &Project, condition: &mut ProjectCondition) {
	let Some(existing) = app.status.as_ref().and_then(|status| {
		status.conditions.iter().find(|existing| {
			existing.type_ == condition.type_ && existing.status == condition.status
		})
	}) else {
		return;
	};

	condition.last_transition_time = existing.last_transition_time.clone();
}

fn has_condition_status(app: &Project, type_: ConditionType, status: ConditionStatus) -> bool {
	app.status.as_ref().is_some_and(|status_value| {
		status_value
			.conditions
			.iter()
			.any(|condition| condition.type_ == type_ && condition.status == status)
	})
}

fn companion_build_condition(
	app: &Project,
	condition: &ProjectCondition,
) -> Option<ProjectCondition> {
	let should_clear_degraded = condition.type_ == ConditionType::Progressing
		&& condition.status == ConditionStatus::True
		&& has_condition_status(app, ConditionType::Degraded, ConditionStatus::True);
	let should_clear_progressing =
		condition.type_ == ConditionType::Degraded && condition.status == ConditionStatus::True;
	let should_clear_degraded_after_success = condition.type_ == ConditionType::Progressing
		&& condition.status == ConditionStatus::False
		&& condition.reason == "BuildSucceeded";

	if should_clear_degraded || should_clear_degraded_after_success {
		return Some(build_condition(
			ConditionType::Degraded,
			ConditionStatus::False,
			&condition.reason,
			&condition.message,
			condition.observed_generation,
		));
	}

	if should_clear_progressing {
		return Some(build_condition(
			ConditionType::Progressing,
			ConditionStatus::False,
			&condition.reason,
			&condition.message,
			condition.observed_generation,
		));
	}

	None
}

pub(crate) fn build_status_patch(
	app: &Project,
	build: BuildStatus,
	mut condition: ProjectCondition,
) -> serde_json::Value {
	let mut companion = companion_build_condition(app, &condition);
	preserve_transition_time_for_unchanged_status(app, &mut condition);
	let mut conditions = app
		.status
		.as_ref()
		.map(|status| status.conditions.clone())
		.unwrap_or_default();
	upsert_condition(&mut conditions, condition);
	if let Some(ref mut companion_condition) = companion {
		preserve_transition_time_for_unchanged_status(app, companion_condition);
		upsert_condition(&mut conditions, companion_condition.clone());
	}

	serde_json::json!({
		"status": {
			"build": build,
			"conditions": conditions,
			"observedGeneration": app.metadata.generation,
		}
	})
}

fn transition_build_status(
	app: &Project,
	build: &mut BuildStatus,
	phase: BuildPhase,
	reason: &str,
	message: &str,
) {
	let preserved_transition_time = app
		.status
		.as_ref()
		.and_then(|status| status.build.as_ref())
		.filter(|existing| {
			existing.phase == phase
				&& existing.trigger == build.trigger
				&& existing.job_name == build.job_name
		})
		.and_then(|existing| existing.last_transition_time.clone());

	build.phase = phase;
	build.reason = Some(reason.to_string());
	build.message = Some(message.to_string());
	build.last_transition_time =
		Some(preserved_transition_time.unwrap_or_else(|| Utc::now().to_rfc3339()));
}

pub(crate) fn clear_build_annotations_patch() -> serde_json::Value {
	serde_json::json!({
		"metadata": {
			"annotations": {
				"reinhardt.dev/build-trigger": null,
				"reinhardt.dev/preview-action": null,
				"reinhardt.dev/pr-number": null,
				"reinhardt.dev/pr-branch": null,
			}
		}
	})
}

pub(crate) fn preview_delete_annotations_patch() -> serde_json::Value {
	clear_build_annotations_patch()
}

fn preview_build_matches(build: &BuildStatus, pr_number: &str, preview_name: &str) -> bool {
	build.target == BuildTargetKind::Preview
		&& ((!pr_number.is_empty() && build.pr_number.as_deref() == Some(pr_number))
			|| build.preview_name.as_deref() == Some(preview_name))
}

fn matching_preview_build_status<'a>(
	app: &'a Project,
	pr_number: &str,
	preview_name: &str,
) -> Option<&'a BuildStatus> {
	app.status
		.as_ref()
		.and_then(|status| status.build.as_ref())
		.filter(|build| preview_build_matches(build, pr_number, preview_name))
}

pub(crate) fn clear_preview_build_status_patch(
	app: &Project,
	pr_number: &str,
	preview_name: &str,
) -> Option<serde_json::Value> {
	matching_preview_build_status(app, pr_number, preview_name)?;

	let message =
		format!("Preview build for {preview_name} was cancelled because the preview was deleted");
	let mut conditions = app
		.status
		.as_ref()
		.map(|status| status.conditions.clone())
		.unwrap_or_default();
	let mut progressing = build_condition(
		ConditionType::Progressing,
		ConditionStatus::False,
		"PreviewDeleted",
		&message,
		app.metadata.generation,
	);
	let mut degraded = build_condition(
		ConditionType::Degraded,
		ConditionStatus::False,
		"PreviewDeleted",
		&message,
		app.metadata.generation,
	);
	preserve_transition_time_for_unchanged_status(app, &mut progressing);
	preserve_transition_time_for_unchanged_status(app, &mut degraded);
	upsert_condition(&mut conditions, progressing);
	upsert_condition(&mut conditions, degraded);

	Some(serde_json::json!({
		"status": {
			"build": null,
			"conditions": conditions,
			"observedGeneration": app.metadata.generation,
		}
	}))
}

async fn patch_build_status(
	client: Client,
	namespace: &str,
	app: &Project,
	build: BuildStatus,
	condition: ProjectCondition,
) -> Result<(), Error> {
	let api: Api<Project> = Api::namespaced(client, namespace);
	let patch = build_status_patch(app, build, condition);
	api.patch_status(
		&app.name_any(),
		&PatchParams::default(),
		&Patch::Merge(&patch),
	)
	.await
	.map_err(Error::Kube)?;
	Ok(())
}

pub(crate) async fn clear_preview_build_for_delete(
	app: &Project,
	client: Client,
	namespace: &str,
	pr_number: &str,
	preview_name: &str,
) -> Result<bool, Error> {
	let Some(build) = matching_preview_build_status(app, pr_number, preview_name).cloned() else {
		return Ok(false);
	};

	let job_api: Api<Job> = Api::namespaced(client.clone(), namespace);
	if !build.job_name.trim().is_empty() {
		match job_api
			.delete(&build.job_name, &DeleteParams::default())
			.await
		{
			Ok(_) => {}
			Err(kube::Error::Api(status)) if status.code == 404 => {}
			Err(error) => return Err(Error::Kube(error)),
		}
	}

	let Some(patch) = clear_preview_build_status_patch(app, pr_number, preview_name) else {
		return Ok(false);
	};
	let api: Api<Project> = Api::namespaced(client, namespace);
	api.patch_status(
		&app.name_any(),
		&PatchParams::default(),
		&Patch::Merge(&patch),
	)
	.await
	.map_err(Error::Kube)?;
	Ok(true)
}

async fn clear_build_annotations(
	client: Client,
	namespace: &str,
	app: &Project,
) -> Result<(), Error> {
	let api: Api<Project> = Api::namespaced(client, namespace);
	let patch = clear_build_annotations_patch();
	api.patch(
		&app.name_any(),
		&PatchParams::default(),
		&Patch::Merge(&patch),
	)
	.await
	.map_err(Error::Kube)?;
	Ok(())
}

fn build_job_for_status(app: &Project, status: &BuildStatus) -> Result<Job, Error> {
	match status.target {
		BuildTargetKind::Production => source::build_kaniko_job(app, &status.image_tag),
		BuildTargetKind::Preview => {
			source::build_kaniko_job_for_branch(app, &status.image_tag, status.branch.as_deref())
		}
	}
}

pub(crate) async fn reconcile_source_build(
	app: &Project,
	client: Client,
	namespace: &str,
) -> Result<BuildDecision, Error> {
	let new_build = derive_new_build_status(app)?;
	let has_new_build = new_build.is_some();
	if let Some(failed_build) = blocking_failed_build_status(app, has_new_build) {
		return Ok(BuildDecision::Failed(BuildFailure {
			status: failed_build,
		}));
	}

	let active_build = active_build_status(app);
	let Some(mut build) = new_build.or(active_build) else {
		return Ok(BuildDecision::NoBuild);
	};

	let job_name = build.job_name.clone();
	let job_api: Api<Job> = Api::namespaced(client.clone(), namespace);
	let live_job = match job_api.get_opt(&job_name).await.map_err(Error::Kube)? {
		Some(job) => job,
		None if has_new_build => {
			let expected_job = build_job_for_status(app, &build)?;
			job_api
				.create(&PostParams::default(), &expected_job)
				.await
				.map_err(Error::Kube)?
		}
		None => {
			let message = format!("Kaniko build Job {job_name} is missing");
			transition_build_status(
				app,
				&mut build,
				BuildPhase::Failed,
				"BuildJobMissing",
				&message,
			);
			let condition = build_condition(
				ConditionType::Degraded,
				ConditionStatus::True,
				"BuildJobMissing",
				&message,
				app.metadata.generation,
			);
			patch_build_status(client, namespace, app, build.clone(), condition).await?;
			return Ok(BuildDecision::Failed(BuildFailure { status: build }));
		}
	};

	if has_new_build {
		let condition = build_condition(
			ConditionType::Progressing,
			ConditionStatus::True,
			"BuildPending",
			"Kaniko build Job has been accepted",
			app.metadata.generation,
		);
		patch_build_status(client.clone(), namespace, app, build.clone(), condition).await?;
		clear_build_annotations(client.clone(), namespace, app).await?;
	}

	match classify_job_state(&live_job) {
		JobBuildState::Running => {
			transition_build_status(
				app,
				&mut build,
				BuildPhase::Running,
				"BuildRunning",
				"Kaniko build Job is still running",
			);
			let condition = build_condition(
				ConditionType::Progressing,
				ConditionStatus::True,
				"BuildRunning",
				"Kaniko build Job is still running",
				app.metadata.generation,
			);
			patch_build_status(client, namespace, app, build, condition).await?;
			Ok(BuildDecision::Waiting {
				requeue_after: Duration::from_secs(BUILD_REQUEUE_SECS),
			})
		}
		JobBuildState::Succeeded => {
			transition_build_status(
				app,
				&mut build,
				BuildPhase::Succeeded,
				"BuildSucceeded",
				"Kaniko build Job succeeded",
			);
			Ok(BuildDecision::Succeeded(BuildCompletion { status: build }))
		}
		JobBuildState::Failed { reason, message } => {
			transition_build_status(app, &mut build, BuildPhase::Failed, &reason, &message);
			let condition = build_condition(
				ConditionType::Degraded,
				ConditionStatus::True,
				"BuildFailed",
				&message,
				app.metadata.generation,
			);
			patch_build_status(client, namespace, app, build.clone(), condition).await?;
			Ok(BuildDecision::Failed(BuildFailure { status: build }))
		}
	}
}

pub(crate) async fn mark_build_succeeded(
	app: &Project,
	client: Client,
	namespace: &str,
	build: BuildStatus,
) -> Result<(), Error> {
	let condition = build_condition(
		ConditionType::Progressing,
		ConditionStatus::False,
		"BuildSucceeded",
		"Kaniko build Job succeeded",
		app.metadata.generation,
	);
	patch_build_status(client, namespace, app, build, condition).await
}

fn annotation_value<'a>(app: &'a Project, key: &str) -> Option<&'a str> {
	app.metadata
		.annotations
		.as_ref()
		.and_then(|annotations| annotations.get(key))
		.map(String::as_str)
}

fn preview_pr_number(app: &Project) -> Option<&str> {
	let action = annotation_value(app, PREVIEW_ACTION_ANNOTATION)?;
	if !matches!(action, "create" | "update") {
		return None;
	}
	annotation_value(app, PR_NUMBER_ANNOTATION)
}

fn image_tag_suffix(image_tag: &str) -> &str {
	let start = image_tag.len().saturating_sub(8);
	&image_tag[start..]
}

#[cfg(test)]
mod tests {
	use k8s_openapi::api::batch::v1::{Job, JobSpec, JobStatus};
	use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
	use reinhardt_cloud_types::crd::{
		BuildPhase, BuildStatus, BuildTargetKind, Project, ProjectStatus,
	};
	use rstest::rstest;

	use super::*;

	fn test_project() -> Project {
		let mut app = serde_json::from_value::<Project>(serde_json::json!({
			"apiVersion": "paas.reinhardt-cloud.dev/v1alpha2",
			"kind": "Project",
			"metadata": {
				"name": "api",
				"namespace": "default",
				"uid": "test-uid"
			},
			"spec": {
				"image": "ghcr.io/acme/api:stable",
				"source": {
					"repository": "https://github.com/acme/api",
					"branch": "main",
					"build": {
						"registry": "ghcr.io/acme/api"
					}
				}
			}
		}))
		.expect("test project should deserialize");
		app.metadata.annotations = Some(std::collections::BTreeMap::from([(
			BUILD_TRIGGER_ANNOTATION.to_string(),
			"abcdef1234567890".to_string(),
		)]));
		app
	}

	fn test_running_build_status() -> BuildStatus {
		BuildStatus {
			phase: BuildPhase::Running,
			target: BuildTargetKind::Production,
			trigger: "abcdef1234567890".to_string(),
			job_name: "api-build-abcdef12".to_string(),
			image: "ghcr.io/acme/api:api-abcdef12".to_string(),
			image_tag: "api-abcdef12".to_string(),
			preview_name: None,
			pr_number: None,
			branch: Some("main".to_string()),
			reason: Some("BuildRunning".to_string()),
			message: Some("Kaniko build Job is running".to_string()),
			started_at: Some("2026-01-01T00:00:00Z".to_string()),
			last_transition_time: Some("2026-01-01T00:00:00Z".to_string()),
		}
	}

	fn test_failed_build_status() -> BuildStatus {
		let mut build = test_running_build_status();
		build.phase = BuildPhase::Failed;
		build.reason = Some("BuildFailed".to_string());
		build.message = Some("Kaniko build Job failed".to_string());
		build
	}

	fn test_preview_build_status() -> BuildStatus {
		let mut build = test_running_build_status();
		build.target = BuildTargetKind::Preview;
		build.image = "ghcr.io/acme/api:pr-42-abcdef12".to_string();
		build.image_tag = "pr-42-abcdef12".to_string();
		build.preview_name = Some("api-pr-42".to_string());
		build.pr_number = Some("42".to_string());
		build.branch = Some("feature/login".to_string());
		build
	}

	fn dummy_client() -> Client {
		use http::Response;
		use http_body_util::Empty;
		use tower::service_fn;

		let svc = service_fn(|_req: http::Request<kube::client::Body>| async {
			Ok::<_, std::convert::Infallible>(Response::builder().body(Empty::new()).unwrap())
		});
		Client::new(svc, "default")
	}

	fn patch_condition<'a>(patch: &'a serde_json::Value, type_: &str) -> &'a serde_json::Value {
		patch["status"]["conditions"]
			.as_array()
			.expect("conditions should be an array")
			.iter()
			.find(|condition| condition["type"] == serde_json::json!(type_))
			.expect("condition should exist")
	}

	#[rstest]
	fn derive_build_status_for_production_trigger() {
		// Arrange
		let app = test_project();

		// Act
		let status = derive_new_build_status(&app)
			.expect("build status should be derived")
			.expect("trigger should create build status");

		// Assert
		assert_eq!(status.phase, BuildPhase::Pending);
		assert_eq!(status.target, BuildTargetKind::Production);
		assert_eq!(status.trigger, "abcdef1234567890");
		assert_eq!(status.job_name, "api-build-abcdef12");
		assert_eq!(status.image_tag, "api-abcdef12");
		assert_eq!(status.image, "ghcr.io/acme/api:api-abcdef12");
		assert_eq!(status.branch.as_deref(), Some("main"));
		assert_eq!(status.preview_name, None);
		assert_eq!(status.pr_number, None);
		assert_eq!(status.reason.as_deref(), Some("BuildPending"));
		assert_eq!(
			status.message.as_deref(),
			Some("Kaniko build Job has been accepted")
		);
		assert!(status.started_at.is_some());
		assert!(status.last_transition_time.is_some());
	}

	#[rstest]
	fn derive_build_status_for_preview_trigger() {
		// Arrange
		let mut app = test_project();
		let annotations = app.metadata.annotations.as_mut().unwrap();
		annotations.insert(PREVIEW_ACTION_ANNOTATION.to_string(), "create".to_string());
		annotations.insert(PR_NUMBER_ANNOTATION.to_string(), "42".to_string());
		annotations.insert(
			PR_BRANCH_ANNOTATION.to_string(),
			"feature/login".to_string(),
		);

		// Act
		let status = derive_new_build_status(&app)
			.expect("build status should be derived")
			.expect("trigger should create build status");

		// Assert
		assert_eq!(status.phase, BuildPhase::Pending);
		assert_eq!(status.target, BuildTargetKind::Preview);
		assert_eq!(status.trigger, "abcdef1234567890");
		assert_eq!(status.job_name, "api-build-abcdef12");
		assert_eq!(status.image_tag, "pr-42-abcdef12");
		assert_eq!(status.image, "ghcr.io/acme/api:pr-42-abcdef12");
		assert_eq!(status.preview_name.as_deref(), Some("api-pr-42"));
		assert_eq!(status.pr_number.as_deref(), Some("42"));
		assert_eq!(status.branch.as_deref(), Some("feature/login"));
	}

	#[rstest]
	fn active_build_status_uses_running_status_without_trigger() {
		// Arrange
		let mut app = test_project();
		app.metadata.annotations = None;
		let running = test_running_build_status();
		app.status = Some(ProjectStatus {
			build: Some(running.clone()),
			..Default::default()
		});

		// Act
		let active = active_build_status(&app);

		// Assert
		assert_eq!(active, Some(running));
	}

	#[rstest]
	#[tokio::test]
	async fn reconcile_source_build_blocks_failed_status_without_trigger() {
		// Arrange
		let mut app = test_project();
		app.metadata.annotations = None;
		let failed = test_failed_build_status();
		app.status = Some(ProjectStatus {
			build: Some(failed.clone()),
			..Default::default()
		});

		// Act
		let decision = reconcile_source_build(&app, dummy_client(), "default")
			.await
			.expect("failed build should produce a decision");

		// Assert
		assert_eq!(
			decision,
			BuildDecision::Failed(BuildFailure { status: failed })
		);
	}

	#[rstest]
	fn blocking_failed_build_status_allows_new_trigger_to_override_failure() {
		// Arrange
		let mut app = test_project();
		let failed = test_failed_build_status();
		app.status = Some(ProjectStatus {
			build: Some(failed),
			..Default::default()
		});
		let has_new_build = derive_new_build_status(&app)
			.expect("new build status should be derived")
			.is_some();

		// Act
		let blocking = blocking_failed_build_status(&app, has_new_build);

		// Assert
		assert!(has_new_build);
		assert_eq!(blocking, None);
	}

	#[rstest]
	fn build_status_patch_preserves_ready_condition_and_adds_progressing() {
		// Arrange
		let mut app = test_project();
		app.metadata.generation = Some(7);
		app.status = Some(ProjectStatus {
			conditions: vec![ProjectCondition {
				type_: ConditionType::Ready,
				status: ConditionStatus::True,
				reason: "ReconcileSucceeded".to_string(),
				message: "Project is ready".to_string(),
				last_transition_time: Some("2026-01-01T00:00:00Z".to_string()),
				observed_generation: Some(6),
			}],
			..Default::default()
		});
		let mut build = derive_new_build_status(&app)
			.expect("build status should be derived")
			.expect("trigger should create build status");
		build.phase = BuildPhase::Running;
		let condition = build_condition(
			ConditionType::Progressing,
			ConditionStatus::True,
			"BuildRunning",
			"Kaniko build Job is still running",
			app.metadata.generation,
		);

		// Act
		let patch = build_status_patch(&app, build, condition);

		// Assert
		assert_eq!(
			patch["status"]["build"]["phase"],
			serde_json::json!("running")
		);
		assert_eq!(patch["status"]["observedGeneration"], serde_json::json!(7));
		let conditions = patch["status"]["conditions"]
			.as_array()
			.expect("conditions should be an array");
		assert_eq!(conditions.len(), 2);
		assert_eq!(conditions[0]["type"], serde_json::json!("Ready"));
		assert_eq!(conditions[0]["status"], serde_json::json!("True"));
		assert_eq!(
			conditions[0]["lastTransitionTime"],
			serde_json::json!("2026-01-01T00:00:00Z")
		);
		assert_eq!(conditions[1]["type"], serde_json::json!("Progressing"));
		assert_eq!(conditions[1]["status"], serde_json::json!("True"));
		assert_eq!(conditions[1]["reason"], serde_json::json!("BuildRunning"));
	}

	#[rstest]
	fn failed_patch_clears_progressing_condition_and_sets_degraded() {
		// Arrange
		let mut app = test_project();
		app.metadata.generation = Some(8);
		app.status = Some(ProjectStatus {
			conditions: vec![ProjectCondition {
				type_: ConditionType::Progressing,
				status: ConditionStatus::True,
				reason: "BuildRunning".to_string(),
				message: "Kaniko build Job is still running".to_string(),
				last_transition_time: Some("2026-01-01T00:00:00Z".to_string()),
				observed_generation: Some(7),
			}],
			..Default::default()
		});
		let mut build = derive_new_build_status(&app)
			.expect("build status should be derived")
			.expect("trigger should create build status");
		transition_build_status(
			&app,
			&mut build,
			BuildPhase::Failed,
			"BackoffLimitExceeded",
			"Kaniko build Job exceeded backoff limit",
		);
		let condition = build_condition(
			ConditionType::Degraded,
			ConditionStatus::True,
			"BuildFailed",
			"Kaniko build Job exceeded backoff limit",
			app.metadata.generation,
		);

		// Act
		let patch = build_status_patch(&app, build, condition);

		// Assert
		let progressing = patch_condition(&patch, "Progressing");
		let degraded = patch_condition(&patch, "Degraded");
		assert_eq!(progressing["status"], serde_json::json!("False"));
		assert_eq!(progressing["reason"], serde_json::json!("BuildFailed"));
		assert_eq!(degraded["status"], serde_json::json!("True"));
		assert_eq!(degraded["reason"], serde_json::json!("BuildFailed"));
	}

	#[rstest]
	fn succeeded_patch_clears_degraded_condition_and_sets_progressing_false() {
		// Arrange
		let mut app = test_project();
		app.metadata.generation = Some(9);
		app.status = Some(ProjectStatus {
			conditions: vec![ProjectCondition {
				type_: ConditionType::Degraded,
				status: ConditionStatus::True,
				reason: "BuildFailed".to_string(),
				message: "Kaniko build Job failed".to_string(),
				last_transition_time: Some("2026-01-01T00:00:00Z".to_string()),
				observed_generation: Some(8),
			}],
			..Default::default()
		});
		let mut build = derive_new_build_status(&app)
			.expect("build status should be derived")
			.expect("trigger should create build status");
		transition_build_status(
			&app,
			&mut build,
			BuildPhase::Succeeded,
			"BuildSucceeded",
			"Kaniko build Job succeeded",
		);
		let condition = build_condition(
			ConditionType::Progressing,
			ConditionStatus::False,
			"BuildSucceeded",
			"Kaniko build Job succeeded",
			app.metadata.generation,
		);

		// Act
		let patch = build_status_patch(&app, build, condition);

		// Assert
		let progressing = patch_condition(&patch, "Progressing");
		let degraded = patch_condition(&patch, "Degraded");
		assert_eq!(progressing["status"], serde_json::json!("False"));
		assert_eq!(progressing["reason"], serde_json::json!("BuildSucceeded"));
		assert_eq!(degraded["status"], serde_json::json!("False"));
		assert_eq!(degraded["reason"], serde_json::json!("BuildSucceeded"));
	}

	#[rstest]
	fn running_transition_preserves_last_transition_time_for_existing_running_build() {
		// Arrange
		let mut app = test_project();
		let running = test_running_build_status();
		app.status = Some(ProjectStatus {
			build: Some(running.clone()),
			..Default::default()
		});
		let mut build = running;

		// Act
		transition_build_status(
			&app,
			&mut build,
			BuildPhase::Running,
			"BuildRunning",
			"Kaniko build Job is still running",
		);

		// Assert
		assert_eq!(build.phase, BuildPhase::Running);
		assert_eq!(build.reason.as_deref(), Some("BuildRunning"));
		assert_eq!(
			build.message.as_deref(),
			Some("Kaniko build Job is still running")
		);
		assert_eq!(
			build.last_transition_time.as_deref(),
			Some("2026-01-01T00:00:00Z")
		);
	}

	#[rstest]
	fn clear_build_annotations_patch_removes_trigger_and_preview_inputs() {
		// Act
		let patch = clear_build_annotations_patch();

		// Assert
		let annotations = &patch["metadata"]["annotations"];
		assert!(annotations["reinhardt.dev/build-trigger"].is_null());
		assert!(annotations["reinhardt.dev/preview-action"].is_null());
		assert!(annotations["reinhardt.dev/pr-number"].is_null());
		assert!(annotations["reinhardt.dev/pr-branch"].is_null());
	}

	#[rstest]
	fn preview_delete_annotations_patch_removes_trigger_and_preview_inputs() {
		// Act
		let patch = preview_delete_annotations_patch();

		// Assert
		let annotations = &patch["metadata"]["annotations"];
		assert!(annotations["reinhardt.dev/build-trigger"].is_null());
		assert!(annotations["reinhardt.dev/preview-action"].is_null());
		assert!(annotations["reinhardt.dev/pr-number"].is_null());
		assert!(annotations["reinhardt.dev/pr-branch"].is_null());
	}

	#[rstest]
	fn clear_preview_build_status_patch_clears_matching_preview_build() {
		// Arrange
		let mut app = test_project();
		app.metadata.generation = Some(10);
		let mut build = test_preview_build_status();
		build.phase = BuildPhase::Succeeded;
		app.status = Some(ProjectStatus {
			build: Some(build),
			conditions: vec![ProjectCondition {
				type_: ConditionType::Ready,
				status: ConditionStatus::True,
				reason: "ReconcileSucceeded".to_string(),
				message: "Project is ready".to_string(),
				last_transition_time: Some("2026-01-01T00:00:00Z".to_string()),
				observed_generation: Some(9),
			}],
			..Default::default()
		});

		// Act
		let patch = clear_preview_build_status_patch(&app, "42", "api-pr-42")
			.expect("matching preview build should produce status patch");

		// Assert
		assert!(patch["status"]["build"].is_null());
		assert_eq!(patch["status"]["observedGeneration"], serde_json::json!(10));
		let progressing = patch_condition(&patch, "Progressing");
		let degraded = patch_condition(&patch, "Degraded");
		assert_eq!(progressing["status"], serde_json::json!("False"));
		assert_eq!(progressing["reason"], serde_json::json!("PreviewDeleted"));
		assert_eq!(degraded["status"], serde_json::json!("False"));
		assert_eq!(degraded["reason"], serde_json::json!("PreviewDeleted"));
		assert_eq!(
			patch_condition(&patch, "Ready")["status"],
			serde_json::json!("True")
		);
	}

	#[rstest]
	fn clear_preview_build_status_patch_ignores_different_preview_build() {
		// Arrange
		let mut app = test_project();
		let build = test_preview_build_status();
		app.status = Some(ProjectStatus {
			build: Some(build),
			..Default::default()
		});

		// Act
		let patch = clear_preview_build_status_patch(&app, "43", "api-pr-43");

		// Assert
		assert_eq!(patch, None);
	}

	#[rstest]
	fn clear_preview_build_status_patch_ignores_production_build() {
		// Arrange
		let mut app = test_project();
		let build = test_running_build_status();
		app.status = Some(ProjectStatus {
			build: Some(build),
			..Default::default()
		});

		// Act
		let patch = clear_preview_build_status_patch(&app, "42", "api-pr-42");

		// Assert
		assert_eq!(patch, None);
	}

	#[rstest]
	fn job_state_reports_success_when_succeeded_count_is_positive() {
		// Arrange
		let job = Job {
			status: Some(JobStatus {
				succeeded: Some(1),
				..Default::default()
			}),
			..Default::default()
		};

		// Act
		let state = classify_job_state(&job);

		// Assert
		assert_eq!(state, JobBuildState::Succeeded);
	}

	#[rstest]
	fn job_state_reports_failure_when_backoff_is_exhausted() {
		// Arrange
		let job = Job {
			metadata: ObjectMeta {
				name: Some("api-build-abcdef12".to_string()),
				..Default::default()
			},
			spec: Some(JobSpec {
				backoff_limit: Some(2),
				..Default::default()
			}),
			status: Some(JobStatus {
				failed: Some(3),
				..Default::default()
			}),
		};

		// Act
		let state = classify_job_state(&job);

		// Assert
		assert_eq!(
			state,
			JobBuildState::Failed {
				reason: "BackoffLimitExceeded".to_string(),
				message: "Kaniko build Job exceeded backoff limit".to_string(),
			}
		);
	}
}
