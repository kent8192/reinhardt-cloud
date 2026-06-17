//! HorizontalPodAutoscaler builder for operator-managed `Project` resources.

use k8s_openapi::api::autoscaling::v2::{
	CrossVersionObjectReference, HorizontalPodAutoscaler, HorizontalPodAutoscalerCondition,
	HorizontalPodAutoscalerSpec, MetricSpec, MetricTarget, ResourceMetricSource,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::ResourceExt;
use reinhardt_cloud_types::crd::Project;
use reinhardt_cloud_types::crd::spec::{ScaleMetric, ScaleSpec};

use super::labels::{Component, owner_reference, standard_labels};
use crate::error::Error;

const DEFAULT_MAX_REPLICAS: i32 = 6;
const DEFAULT_TARGET_VALUE: i32 = 70;

pub(crate) enum AutoscalerPlan {
	Apply(Box<HorizontalPodAutoscaler>),
	Unsupported {
		reason: &'static str,
		message: String,
	},
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedScale {
	min_replicas: i32,
	max_replicas: i32,
	metric: ScaleMetric,
	target_value: i32,
}

fn resolve_scale(app: &Project, scale: &ScaleSpec) -> ResolvedScale {
	let min_replicas = scale.min_replicas.or(app.spec.replicas).unwrap_or(1);
	let max_replicas = scale
		.max_replicas
		.unwrap_or_else(|| DEFAULT_MAX_REPLICAS.max(min_replicas));
	let metric = scale.metric.clone().unwrap_or(ScaleMetric::Cpu);
	let target_value = scale.target_value.unwrap_or(DEFAULT_TARGET_VALUE);

	ResolvedScale {
		min_replicas,
		max_replicas,
		metric,
		target_value,
	}
}

pub(crate) fn build_autoscaler(app: &Project) -> Result<Option<AutoscalerPlan>, Error> {
	let Some(scale) = app.spec.scale.as_ref() else {
		return Ok(None);
	};

	let resolved = resolve_scale(app, scale);
	if matches!(resolved.metric, ScaleMetric::Rps) {
		return Ok(Some(AutoscalerPlan::Unsupported {
			reason: "UnsupportedMetric",
			message: "RPS autoscaling requires custom or external metrics and is not supported by the standard HPA implementation".to_string(),
		}));
	}

	let name = app.name_any();
	let namespace = super::require_namespace(app)?;
	let labels = standard_labels(app, Component::Web);
	let owner_ref = owner_reference(app)?;
	let metric = build_metric(&resolved);

	Ok(Some(AutoscalerPlan::Apply(Box::new(
		HorizontalPodAutoscaler {
			metadata: ObjectMeta {
				name: Some(name.clone()),
				namespace: Some(namespace),
				labels: Some(labels),
				owner_references: Some(vec![owner_ref]),
				..Default::default()
			},
			spec: Some(HorizontalPodAutoscalerSpec {
				scale_target_ref: CrossVersionObjectReference {
					api_version: Some("apps/v1".to_string()),
					kind: "Deployment".to_string(),
					name,
				},
				min_replicas: Some(resolved.min_replicas),
				max_replicas: resolved.max_replicas,
				metrics: Some(vec![metric]),
				..Default::default()
			}),
			status: None,
		},
	))))
}

fn build_metric(scale: &ResolvedScale) -> MetricSpec {
	let (name, target) = match scale.metric {
		ScaleMetric::Cpu => (
			"cpu",
			MetricTarget {
				type_: "Utilization".to_string(),
				average_utilization: Some(scale.target_value),
				..Default::default()
			},
		),
		ScaleMetric::Memory => (
			"memory",
			MetricTarget {
				type_: "AverageValue".to_string(),
				average_value: Some(Quantity(format!("{}Mi", scale.target_value))),
				..Default::default()
			},
		),
		ScaleMetric::Rps => unreachable!("RPS is handled before metric construction"),
	};

	MetricSpec {
		type_: "Resource".to_string(),
		resource: Some(ResourceMetricSource {
			name: name.to_string(),
			target,
		}),
		..Default::default()
	}
}

pub(crate) fn hpa_is_ready(hpa: &HorizontalPodAutoscaler) -> bool {
	let observed = hpa
		.status
		.as_ref()
		.and_then(|status| status.observed_generation)
		.zip(hpa.metadata.generation)
		.is_some_and(|(observed, generation)| observed >= generation);
	if !observed {
		return false;
	}

	let conditions = hpa
		.status
		.as_ref()
		.and_then(|status| status.conditions.as_ref())
		.map(Vec::as_slice)
		.unwrap_or(&[]);

	has_true_condition(conditions, "AbleToScale") && has_true_condition(conditions, "ScalingActive")
}

fn has_true_condition(conditions: &[HorizontalPodAutoscalerCondition], type_: &str) -> bool {
	conditions
		.iter()
		.any(|condition| condition.type_ == type_ && condition.status == "True")
}

#[cfg(test)]
mod tests {
	use super::*;
	use k8s_openapi::api::autoscaling::v2::HorizontalPodAutoscalerStatus;
	use kube::api::ObjectMeta;
	use reinhardt_cloud_types::crd::spec::{ScaleMetric, ScaleSpec};
	use reinhardt_cloud_types::crd::{Project, ProjectSpec};
	use rstest::rstest;

	fn make_test_app(name: &str) -> Project {
		Project {
			metadata: ObjectMeta {
				name: Some(name.to_string()),
				namespace: Some("default".to_string()),
				uid: Some("test-uid-12345".to_string()),
				..Default::default()
			},
			spec: ProjectSpec {
				image: "img:v1".to_string(),
				replicas: Some(2),
				..Default::default()
			},
			status: None,
		}
	}

	#[rstest]
	fn build_hpa_uses_cpu_utilization_metric() {
		// Arrange
		let mut app = make_test_app("web");
		app.spec.scale = Some(ScaleSpec {
			min_replicas: Some(2),
			max_replicas: Some(8),
			metric: Some(ScaleMetric::Cpu),
			target_value: Some(60),
		});

		// Act
		let plan = build_autoscaler(&app)
			.expect("builder should succeed")
			.expect("scale should create a plan");

		// Assert
		let hpa = match plan {
			AutoscalerPlan::Apply(hpa) => *hpa,
			AutoscalerPlan::Unsupported { .. } => panic!("cpu should be supported"),
		};
		let spec = hpa.spec.expect("spec");
		assert_eq!(spec.min_replicas, Some(2));
		assert_eq!(spec.max_replicas, 8);
		assert_eq!(spec.scale_target_ref.name, "web");
		let metric = &spec.metrics.as_ref().expect("metrics")[0];
		assert_eq!(metric.type_, "Resource");
		let resource = metric.resource.as_ref().expect("resource");
		assert_eq!(resource.name, "cpu");
		assert_eq!(resource.target.type_, "Utilization");
		assert_eq!(resource.target.average_utilization, Some(60));
	}

	#[rstest]
	fn build_hpa_uses_memory_average_value_in_mib() {
		// Arrange
		let mut app = make_test_app("web");
		app.spec.scale = Some(ScaleSpec {
			min_replicas: Some(1),
			max_replicas: Some(4),
			metric: Some(ScaleMetric::Memory),
			target_value: Some(512),
		});

		// Act
		let plan = build_autoscaler(&app)
			.expect("builder should succeed")
			.expect("scale should create a plan");

		// Assert
		let hpa = match plan {
			AutoscalerPlan::Apply(hpa) => *hpa,
			AutoscalerPlan::Unsupported { .. } => panic!("memory should be supported"),
		};
		let metric = &hpa.spec.unwrap().metrics.unwrap()[0];
		let resource = metric.resource.as_ref().expect("resource");
		assert_eq!(resource.name, "memory");
		assert_eq!(resource.target.type_, "AverageValue");
		assert_eq!(resource.target.average_value.as_ref().unwrap().0, "512Mi");
	}

	#[rstest]
	fn build_hpa_defaults_partial_scale() {
		// Arrange
		let mut app = make_test_app("web");
		app.spec.scale = Some(ScaleSpec {
			min_replicas: None,
			max_replicas: None,
			metric: None,
			target_value: None,
		});

		// Act
		let plan = build_autoscaler(&app)
			.expect("builder should succeed")
			.expect("scale should create a plan");

		// Assert
		let hpa = match plan {
			AutoscalerPlan::Apply(hpa) => *hpa,
			AutoscalerPlan::Unsupported { .. } => panic!("default cpu should be supported"),
		};
		let spec = hpa.spec.expect("spec");
		assert_eq!(spec.min_replicas, Some(2));
		assert_eq!(spec.max_replicas, 6);
		let resource = spec.metrics.unwrap()[0].resource.clone().unwrap();
		assert_eq!(resource.name, "cpu");
		assert_eq!(resource.target.average_utilization, Some(70));
	}

	#[rstest]
	fn build_hpa_reports_rps_as_unsupported() {
		// Arrange
		let mut app = make_test_app("web");
		app.spec.scale = Some(ScaleSpec {
			min_replicas: Some(1),
			max_replicas: Some(4),
			metric: Some(ScaleMetric::Rps),
			target_value: Some(100),
		});

		// Act
		let plan = build_autoscaler(&app)
			.expect("builder should succeed")
			.expect("scale should create a plan");

		// Assert
		assert!(matches!(
			plan,
			AutoscalerPlan::Unsupported {
				reason: "UnsupportedMetric",
				..
			}
		));
	}

	#[rstest]
	fn hpa_ready_requires_observed_generation_and_active_conditions() {
		// Arrange
		let mut app = make_test_app("web");
		app.metadata.generation = Some(3);
		app.spec.scale = Some(ScaleSpec {
			min_replicas: Some(1),
			max_replicas: Some(4),
			metric: Some(ScaleMetric::Cpu),
			target_value: Some(70),
		});
		let mut hpa = match build_autoscaler(&app).unwrap().unwrap() {
			AutoscalerPlan::Apply(hpa) => *hpa,
			AutoscalerPlan::Unsupported { .. } => panic!("cpu should be supported"),
		};
		hpa.metadata.generation = Some(3);
		hpa.status = Some(HorizontalPodAutoscalerStatus {
			observed_generation: Some(3),
			conditions: Some(vec![
				HorizontalPodAutoscalerCondition {
					type_: "AbleToScale".to_string(),
					status: "True".to_string(),
					..Default::default()
				},
				HorizontalPodAutoscalerCondition {
					type_: "ScalingActive".to_string(),
					status: "True".to_string(),
					..Default::default()
				},
			]),
			..Default::default()
		});

		// Act
		let ready = hpa_is_ready(&hpa);

		// Assert
		assert!(ready);
	}

	#[rstest]
	fn hpa_not_ready_when_scaling_active_false() {
		// Arrange
		let mut app = make_test_app("web");
		app.metadata.generation = Some(3);
		app.spec.scale = Some(ScaleSpec {
			min_replicas: Some(1),
			max_replicas: Some(4),
			metric: Some(ScaleMetric::Cpu),
			target_value: Some(70),
		});
		let mut hpa = match build_autoscaler(&app).unwrap().unwrap() {
			AutoscalerPlan::Apply(hpa) => *hpa,
			AutoscalerPlan::Unsupported { .. } => panic!("cpu should be supported"),
		};
		hpa.metadata.generation = Some(3);
		hpa.status = Some(HorizontalPodAutoscalerStatus {
			observed_generation: Some(3),
			conditions: Some(vec![
				HorizontalPodAutoscalerCondition {
					type_: "AbleToScale".to_string(),
					status: "True".to_string(),
					..Default::default()
				},
				HorizontalPodAutoscalerCondition {
					type_: "ScalingActive".to_string(),
					status: "False".to_string(),
					..Default::default()
				},
			]),
			..Default::default()
		});

		// Act
		let ready = hpa_is_ready(&hpa);

		// Assert
		assert!(!ready);
	}
}
