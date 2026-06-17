// Task 3 wires this Task 2 helper surface into reconciliation.
#![allow(dead_code)]

use std::time::Duration;

use chrono::Utc;
use k8s_openapi::api::batch::v1::Job;
use kube::ResourceExt;
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

pub(crate) fn derive_new_build_status(app: &Project) -> Result<Option<BuildStatus>, Error> {
	let Some(source_spec) = app.spec.source.as_ref() else {
		return Ok(None);
	};
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
		let running = BuildStatus {
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
		};
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
