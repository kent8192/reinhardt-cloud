//! Maps child preview `Project`s into `PreviewStatus` records for the parent.
//!
//! The reconciler lists the live preview child Projects in the `{parent}-preview`
//! namespace and calls [`build_preview_status_list`] to fold them into the
//! `previews` field of the parent `ProjectStatus`, which the Dashboard reads.

use kube::ResourceExt;
use reinhardt_cloud_types::crd::Project;
use reinhardt_cloud_types::crd::status::PreviewStatus;

/// Builds the `previews` vector for a parent status patch from the live set of
/// child preview Projects. Pure function; does not call the API.
///
/// `url_scheme` prefixes the resolved preview host (e.g. `"https"`) so the
/// Dashboard gets a clickable URL.
pub(crate) fn build_preview_status_list(
	previews: &[Project],
	url_scheme: &str,
) -> Vec<PreviewStatus> {
	previews
		.iter()
		.map(|project| {
			let host = project
				.spec
				.services
				.as_ref()
				.and_then(|services| services.ingress_host.as_deref());
			let url = host.map(|h| format!("{url_scheme}://{h}"));
			let pr_number = project
				.metadata
				.labels
				.as_ref()
				.and_then(|labels| labels.get("reinhardt.dev/pr-number"))
				.cloned()
				.unwrap_or_default();
			let last_activity = project
				.metadata
				.annotations
				.as_ref()
				.and_then(|annotations| annotations.get("reinhardt.dev/last-activity"))
				.cloned();
			let phase = project
				.status
				.as_ref()
				.and_then(|status| status.phase.clone());
			let ready_replicas = project
				.status
				.as_ref()
				.and_then(|status| status.ready_replicas);
			PreviewStatus {
				name: project.name_any(),
				pr_number,
				url,
				phase,
				ready_replicas,
				last_activity,
			}
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;
	use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
	use reinhardt_cloud_types::crd::{ProjectPhase, ProjectSpec, ProjectStatus, ServicesSpec};
	use rstest::rstest;

	#[rstest]
	fn maps_preview_to_status_with_url_and_phase() {
		// Arrange
		let project = Project {
			metadata: ObjectMeta {
				name: Some("my-app-pr-42".to_string()),
				labels: Some(
					[("reinhardt.dev/pr-number".to_string(), "42".to_string())]
						.into_iter()
						.collect(),
				),
				annotations: Some(
					[(
						"reinhardt.dev/last-activity".to_string(),
						"2026-06-18T00:00:00Z".to_string(),
					)]
					.into_iter()
					.collect(),
				),
				..Default::default()
			},
			spec: ProjectSpec {
				services: Some(ServicesSpec {
					port: Some(80),
					target_port: Some(8080),
					ingress_host: Some("my-app-pr-42.preview.example.com".to_string()),
					tls: None,
				}),
				..Default::default()
			},
			status: Some(ProjectStatus {
				phase: Some(ProjectPhase::Running),
				ready_replicas: Some(1),
				..Default::default()
			}),
		};

		// Act
		let list = build_preview_status_list(&[project], "https");

		// Assert
		assert_eq!(list.len(), 1);
		assert_eq!(list[0].name, "my-app-pr-42");
		assert_eq!(list[0].pr_number, "42");
		assert_eq!(
			list[0].url.as_deref(),
			Some("https://my-app-pr-42.preview.example.com")
		);
		assert_eq!(list[0].phase, Some(ProjectPhase::Running));
		assert_eq!(list[0].ready_replicas, Some(1));
		assert_eq!(
			list[0].last_activity.as_deref(),
			Some("2026-06-18T00:00:00Z")
		);
	}

	#[rstest]
	fn omits_url_when_no_ingress_host() {
		// Arrange — a preview Project without an ingress host.
		let project = Project {
			metadata: ObjectMeta {
				name: Some("my-app-pr-7".to_string()),
				labels: Some(
					[("reinhardt.dev/pr-number".to_string(), "7".to_string())]
						.into_iter()
						.collect(),
				),
				..Default::default()
			},
			spec: ProjectSpec {
				services: Some(ServicesSpec {
					port: Some(80),
					target_port: Some(8080),
					ingress_host: None,
					tls: None,
				}),
				..Default::default()
			},
			status: None,
		};

		// Act
		let list = build_preview_status_list(&[project], "https");

		// Assert
		assert_eq!(list.len(), 1);
		assert!(list[0].url.is_none());
		assert_eq!(list[0].pr_number, "7");
	}
}
