//! Spec types for the `Project` custom resource.

use std::collections::BTreeMap;
use std::path::Path;

use k8s_openapi::api::core::v1::LocalObjectReference;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::introspect::IntrospectOutput;
use crate::validation::ValidationError;

use super::auth::AuthSpec;
use super::cache::CacheSpec;
use super::database::DatabaseSpec;
use super::infrastructure::InfrastructureSpec;
use super::isolation::IsolationSpec;
use super::mail::MailSpec;
use super::pages::PagesSpec;
use super::plugins::{PluginSpec, sanitized_volume_suffix};
use super::policy::DeletionPolicy;
use super::service_account::ServiceAccountSpec;
use super::source::SourceSpec;
use super::status::ProjectStatus;
use super::storage::StorageSpec;
use super::tenant::TenantRef;
use super::worker::WorkerSpec;

/// Metric type for autoscaling.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum ScaleMetric {
	Cpu,
	Memory,
	Rps,
}

/// Autoscaling configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ScaleSpec {
	/// Minimum number of replicas
	pub min_replicas: Option<i32>,
	/// Maximum number of replicas
	pub max_replicas: Option<i32>,
	/// Metric to scale on
	pub metric: Option<ScaleMetric>,
	/// Target value for the scaling metric
	pub target_value: Option<i32>,
}

impl ScaleSpec {
	/// Validates the autoscaling specification.
	///
	/// Checks that replica counts are positive, max >= min when both
	/// are present, and target_value is positive.
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if let Some(min) = self.min_replicas
			&& min < 1
		{
			errors.push(ValidationError::new("scale.min_replicas must be >= 1"));
		}

		if let Some(max) = self.max_replicas
			&& max < 1
		{
			errors.push(ValidationError::new("scale.max_replicas must be >= 1"));
		}

		if let (Some(min), Some(max)) = (self.min_replicas, self.max_replicas)
			&& max < min
		{
			errors.push(ValidationError::new(
				"scale.max_replicas must be >= scale.min_replicas",
			));
		}

		if let Some(target) = self.target_value
			&& target <= 0
		{
			errors.push(ValidationError::new("scale.target_value must be > 0"));
		}

		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}
}

/// Health check configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct HealthSpec {
	/// HTTP path for health checks
	pub path: Option<String>,
	/// Port for health checks
	pub port: Option<i32>,
	/// Interval between health checks in seconds
	pub interval_seconds: Option<i32>,
}

impl HealthSpec {
	/// Validates the health check specification.
	///
	/// Checks that port is within the valid range (1-65535) and
	/// interval_seconds is positive.
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if let Some(port) = self.port
			&& !(1..=65535).contains(&port)
		{
			errors.push(ValidationError::new(
				"health.port must be between 1 and 65535",
			));
		}

		if let Some(interval) = self.interval_seconds
			&& interval <= 0
		{
			errors.push(ValidationError::new("health.interval_seconds must be > 0"));
		}

		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}
}

/// TLS configuration for generated Ingress resources.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ServiceTlsSpec {
	/// Whether TLS should be configured on the generated Ingress
	#[serde(default)]
	pub enabled: bool,
	/// Secret containing the certificate and private key
	pub secret_name: Option<String>,
	/// cert-manager Issuer name in the same namespace
	pub issuer: Option<String>,
	/// Unsupported cert-manager ClusterIssuer name; use `issuer` for a namespace-scoped Issuer
	pub cluster_issuer: Option<String>,
}

impl ServiceTlsSpec {
	/// Validates TLS settings against the surrounding service exposure config.
	pub fn validate(&self, ingress_host: Option<&str>) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if self.enabled
			&& ingress_host
				.map(str::trim)
				.filter(|host| !host.is_empty())
				.is_none()
		{
			errors.push(ValidationError::new(
				"services.tls.enabled requires services.ingress_host",
			));
		}

		if self.enabled
			&& self
				.secret_name
				.as_deref()
				.map(str::is_empty)
				.unwrap_or(true)
		{
			errors.push(ValidationError::new(
				"services.tls.secret_name is required when services.tls.enabled is true",
			));
		}

		if self.cluster_issuer.is_some() {
			errors.push(ValidationError::new(
				"services.tls.cluster_issuer is not supported; use services.tls.issuer with a namespace-scoped Issuer",
			));
		}

		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}
}

/// Service exposure configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ServicesSpec {
	/// Service port
	pub port: Option<i32>,
	/// Target port on the container
	pub target_port: Option<i32>,
	/// Ingress hostname for external access
	pub ingress_host: Option<String>,
	/// TLS configuration for generated Ingress resources
	pub tls: Option<ServiceTlsSpec>,
}

impl ServicesSpec {
	/// Validates the service exposure specification.
	///
	/// Checks that port and target_port are within the valid range (1-65535).
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if let Some(port) = self.port
			&& !(1..=65535).contains(&port)
		{
			errors.push(ValidationError::new(
				"services.port must be between 1 and 65535",
			));
		}

		if let Some(target_port) = self.target_port
			&& !(1..=65535).contains(&target_port)
		{
			errors.push(ValidationError::new(
				"services.target_port must be between 1 and 65535",
			));
		}

		if let Some(ref tls) = self.tls
			&& let Err(errs) = tls.validate(self.ingress_host.as_deref())
		{
			errors.extend(errs);
		}

		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}
}

/// Spec for the `Project` custom resource.
#[derive(CustomResource, Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[kube(
	group = "paas.reinhardt-cloud.dev",
	version = "v1alpha2",
	kind = "Project",
	namespaced,
	status = "ProjectStatus",
	printcolumn = r#"{"name":"Image","type":"string","jsonPath":".spec.image"}"#,
	printcolumn = r#"{"name":"Replicas","type":"integer","jsonPath":".spec.replicas"}"#,
	printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#,
	printcolumn = r#"{"name":"Ready","type":"string","jsonPath":".status.conditions[?(@.type==\"Ready\")].status"}"#
)]
pub struct ProjectSpec {
	/// Docker image to deploy
	pub image: String,
	/// Number of desired replicas (defaults to 1)
	pub replicas: Option<i32>,
	/// Database infrastructure configuration
	pub database: Option<DatabaseSpec>,
	/// Cache configuration (Redis)
	pub cache: Option<CacheSpec>,
	/// Background worker configuration
	pub worker: Option<WorkerSpec>,
	/// Authentication configuration
	pub auth: Option<AuthSpec>,
	/// Object storage configuration
	pub storage: Option<StorageSpec>,
	/// Mail (SMTP) configuration
	pub mail: Option<MailSpec>,
	/// Autoscaling configuration
	pub scale: Option<ScaleSpec>,
	/// Health check configuration
	pub health: Option<HealthSpec>,
	/// Service exposure configuration
	pub services: Option<ServicesSpec>,
	/// Cloud resource deletion policy (defaults to Retain)
	#[serde(default)]
	pub deletion_policy: DeletionPolicy,
	/// Resolved reinhardt-web feature flags
	#[serde(default)]
	pub features: Vec<String>,
	/// Environment variables as key-value pairs
	#[serde(default)]
	pub env: BTreeMap<String, String>,
	/// reinhardt-pages frontend configuration.
	/// Auto-detected from introspect when not explicitly set.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub pages: Option<PagesSpec>,
	/// Introspect metadata from `manage introspect` output.
	/// When present, the operator uses this to infer infrastructure requirements.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub introspect: Option<IntrospectOutput>,
	/// Workload isolation and security configuration.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub isolation: Option<IsolationSpec>,
	/// Git source and CI/CD pipeline configuration.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub source: Option<SourceSpec>,
	/// dentdelion WASM plugins to attach to the application.
	///
	/// Each entry produces a `dentdelion.toml` `[[plugins]]` section
	/// and is mounted into the container via a volume at `wasm_dir`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub plugins: Option<Vec<PluginSpec>>,
	/// References to Kubernetes Secrets in the same namespace that hold
	/// container-registry credentials (type `kubernetes.io/dockerconfigjson`).
	///
	/// Operator-generated workload `PodSpec` values only accept app-owned
	/// secret names that start with the app's `{metadata.name}-` prefix.
	/// Operator-created previews may also use verified parent-app prefixes,
	/// which lets previews inherit registry access without allowing arbitrary
	/// `Project` names to borrow shared namespace registry credentials. Legacy
	/// previews without the parent namespace label are accepted only when their
	/// namespace matches the canonical legacy preview contract.
	#[serde(
		rename = "imagePullSecrets",
		default,
		skip_serializing_if = "Option::is_none"
	)]
	pub image_pull_secrets: Option<Vec<LocalObjectReference>>,
	/// Per-app Kubernetes ServiceAccount configuration.
	///
	/// Configures the workload's KSA — typically annotated with GKE
	/// Workload Identity or AWS IRSA bindings to grant the application
	/// pods cloud-API access. Distinct from the operator-managed
	/// `{app-name}-storage` KSA used for storage-backend access.
	#[serde(rename = "serviceAccount", skip_serializing_if = "Option::is_none")]
	pub service_account: Option<ServiceAccountSpec>,

	/// Per-app managed cloud resources (Postgres, buckets, DNS, secrets).
	///
	/// When present, `reinhardt-cloud terraform generate` reads this block
	/// and emits provider-scoped HCL so that per-app infrastructure stays
	/// in sync with the CRD spec. Omit this field for apps that rely
	/// solely on cluster-level shared infrastructure provisioned by #411.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub infrastructure: Option<InfrastructureSpec>,

	/// Multi-tenant ownership marker (Refs #416).
	///
	/// Identifies the owning Organization (and optionally a Team) for this
	/// `Project`. The operator computes a deterministic Kubernetes
	/// namespace per tenant — `tenant-{organization}` or
	/// `tenant-{organization}-{team}` — applies tenant-scoped
	/// `ResourceQuota` and `NetworkPolicy` resources, and rejects CRs whose
	/// `metadata.namespace` does not match the computed value (Degraded
	/// status with reason `TenantMismatch`).
	///
	/// Optional for backward compatibility with `v1alpha1`-style CRs. When
	/// `tenant` is `None`, the operator falls back to the legacy behavior of
	/// honoring whatever namespace was set externally without enforcing any
	/// inter-tenant boundary. New CRs SHOULD always set this field; the
	/// option will become required in `v1alpha3`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub tenant: Option<TenantRef>,
}

impl ProjectSpec {
	/// Validates the full application specification.
	///
	/// Checks replicas and delegates to nested spec validations,
	/// collecting all errors.
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if let Some(replicas) = self.replicas
			&& replicas < 0
		{
			errors.push(ValidationError::new("spec.replicas must be >= 0"));
		}

		if let Some(ref tenant) = self.tenant
			&& let Err(errs) = tenant.validate()
		{
			for e in errs {
				errors.push(ValidationError::new(format!("spec.{}", e.message)));
			}
		}

		if let Some(ref db) = self.database
			&& let Err(msg) = db.validate()
		{
			errors.push(ValidationError::new(msg));
		}

		if let Some(ref w) = self.worker
			&& let Err(msg) = w.validate()
		{
			errors.push(ValidationError::new(msg));
		}

		if let Some(ref m) = self.mail
			&& let Err(msg) = m.validate()
		{
			errors.push(ValidationError::new(msg));
		}

		if let Some(ref scale) = self.scale
			&& let Err(errs) = scale.validate()
		{
			errors.extend(errs);
		}

		if let Some(ref health) = self.health
			&& let Err(errs) = health.validate()
		{
			errors.extend(errs);
		}

		if let Some(ref services) = self.services
			&& let Err(errs) = services.validate()
		{
			errors.extend(errs);
		}

		if let Some(ref pages) = self.pages
			&& let Err(errs) = pages.validate()
		{
			errors.extend(errs);
		}

		if let Some(ref isolation) = self.isolation
			&& let Err(errs) = isolation.validate()
		{
			errors.extend(errs);
		}

		if let Some(ref source) = self.source
			&& let Err(errs) = source.validate()
		{
			errors.extend(errs);
		}

		if let Some(ref sa) = self.service_account
			&& let Err(errs) = sa.validate()
		{
			errors.extend(errs);
		}

		if let Some(ref infra) = self.infrastructure
			&& let Err(errs) = infra.validate()
		{
			errors.extend(errs);
		}

		if let Some(ref plugins) = self.plugins {
			for plugin in plugins {
				if let Err(errs) = plugin.validate() {
					errors.extend(errs);
				}
			}

			// Cross-entry uniqueness checks. Two plugins whose names
			// sanitize to the same Volume suffix would collide when the
			// operator materializes the PodSpec, and two plugins sharing
			// the same wasm_dir would produce duplicate VolumeMount
			// mount paths. Both are rejected by kubelet at admission, so
			// surface them here as validation errors instead.
			let mut seen_suffixes: std::collections::BTreeSet<String> =
				std::collections::BTreeSet::new();
			let mut seen_dirs: std::collections::BTreeSet<String> =
				std::collections::BTreeSet::new();
			for plugin in plugins {
				let suffix = sanitized_volume_suffix(&plugin.name);
				if !seen_suffixes.insert(suffix.clone()) {
					errors.push(ValidationError::new(format!(
						"spec.plugins contains entries whose sanitized name collides on Volume suffix '{suffix}'"
					)));
				}
				let dir = plugin.wasm_dir.trim().to_string();
				if dir.is_empty() {
					continue;
				}
				if !seen_dirs.insert(dir.clone()) {
					errors.push(ValidationError::new(format!(
						"spec.plugins contains entries with duplicate wasm_dir '{dir}'"
					)));
				}
			}
			let dirs: Vec<&str> = seen_dirs.iter().map(String::as_str).collect();
			for (index, dir) in dirs.iter().enumerate() {
				let dir_path = Path::new(dir);
				for other in dirs.iter().skip(index + 1) {
					let other_path = Path::new(other);
					if dir_path.starts_with(other_path) || other_path.starts_with(dir_path) {
						errors.push(ValidationError::new(format!(
							"spec.plugins contains overlapping wasm_dir mount paths '{dir}' and '{other}'"
						)));
					}
				}
			}
		}

		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::crd::database::DatabaseEngine;
	use rstest::rstest;

	#[rstest]
	fn crd_spec_serialization_roundtrip() {
		// Arrange
		let spec = ProjectSpec {
			image: "myapp:latest".to_string(),
			replicas: Some(3),
			database: Some(DatabaseSpec {
				engine: DatabaseEngine::Postgresql,
				instance_class: None,
				storage_gb: Some(20),
				version: Some("16".to_string()),
			}),
			cache: None,
			worker: None,
			auth: None,
			storage: None,
			mail: None,
			scale: Some(ScaleSpec {
				min_replicas: Some(1),
				max_replicas: Some(10),
				metric: Some(ScaleMetric::Cpu),
				target_value: Some(80),
			}),
			health: Some(HealthSpec {
				path: Some("/healthz".to_string()),
				port: Some(8080),
				interval_seconds: Some(30),
			}),
			services: Some(ServicesSpec {
				port: Some(80),
				target_port: Some(8080),
				ingress_host: Some("myapp.example.com".to_string()),
				tls: None,
			}),
			pages: None,
			deletion_policy: DeletionPolicy::default(),
			features: vec![],
			isolation: None,
			source: None,
			env: BTreeMap::from([
				("RUST_LOG".to_string(), "info".to_string()),
				(
					"DATABASE_URL".to_string(),
					"postgres://localhost/db".to_string(),
				),
			]),
			introspect: None,
			plugins: None,
			image_pull_secrets: None,
			service_account: None,
			infrastructure: None,
			tenant: None,
		};

		// Act
		let json = serde_json::to_string(&spec).expect("serialization should succeed");
		let deserialized: ProjectSpec =
			serde_json::from_str(&json).expect("deserialization should succeed");

		// Assert
		assert_eq!(deserialized.image, "myapp:latest");
		assert_eq!(deserialized.replicas, Some(3));
		assert_eq!(
			deserialized.database.unwrap().engine,
			DatabaseEngine::Postgresql
		);
		assert_eq!(deserialized.env.len(), 2);
		assert_eq!(deserialized.env.get("RUST_LOG").unwrap(), "info");
	}

	#[rstest]
	fn crd_spec_defaults() {
		// Arrange
		let json = r#"{"image": "myapp:v1"}"#;

		// Act
		let spec: ProjectSpec = serde_json::from_str(json).expect("deserialization should succeed");

		// Assert
		assert_eq!(spec.image, "myapp:v1");
		assert_eq!(spec.replicas, None);
		assert_eq!(spec.database, None);
		assert_eq!(spec.cache, None);
		assert_eq!(spec.worker, None);
		assert_eq!(spec.auth, None);
		assert_eq!(spec.storage, None);
		assert_eq!(spec.mail, None);
		assert_eq!(spec.scale, None);
		assert_eq!(spec.health, None);
		assert_eq!(spec.services, None);
		assert_eq!(spec.deletion_policy, DeletionPolicy::Retain);
		assert!(spec.features.is_empty());
		assert!(spec.env.is_empty());
		assert!(spec.pages.is_none());
		assert!(spec.introspect.is_none());
	}

	#[rstest]
	fn scale_spec_validation_valid() {
		// Arrange
		let spec = ScaleSpec {
			min_replicas: Some(1),
			max_replicas: Some(10),
			metric: Some(ScaleMetric::Cpu),
			target_value: Some(80),
		};

		// Act
		let result = spec.validate();

		// Assert
		assert!(result.is_ok());
	}

	#[rstest]
	fn scale_spec_validation_negative_replicas() {
		// Arrange
		let spec = ScaleSpec {
			min_replicas: Some(-1),
			max_replicas: Some(10),
			metric: None,
			target_value: None,
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.unwrap_err();
		assert_eq!(errors.len(), 1);
		assert_eq!(errors[0].message, "scale.min_replicas must be >= 1");
	}

	#[rstest]
	fn scale_spec_validation_zero_replicas() {
		// Arrange
		let spec = ScaleSpec {
			min_replicas: Some(0),
			max_replicas: Some(0),
			metric: None,
			target_value: None,
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.unwrap_err();
		assert_eq!(errors.len(), 2);
		assert_eq!(errors[0].message, "scale.min_replicas must be >= 1");
		assert_eq!(errors[1].message, "scale.max_replicas must be >= 1");
	}

	#[rstest]
	fn scale_spec_validation_max_less_than_min() {
		// Arrange
		let spec = ScaleSpec {
			min_replicas: Some(10),
			max_replicas: Some(5),
			metric: None,
			target_value: None,
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.unwrap_err();
		assert_eq!(errors.len(), 1);
		assert_eq!(
			errors[0].message,
			"scale.max_replicas must be >= scale.min_replicas"
		);
	}

	#[rstest]
	fn health_spec_validation_invalid_port() {
		// Arrange
		let spec_zero = HealthSpec {
			path: None,
			port: Some(0),
			interval_seconds: None,
		};
		let spec_over = HealthSpec {
			path: None,
			port: Some(65536),
			interval_seconds: None,
		};
		let spec_negative = HealthSpec {
			path: None,
			port: Some(-1),
			interval_seconds: None,
		};

		// Act
		let result_zero = spec_zero.validate();
		let result_over = spec_over.validate();
		let result_negative = spec_negative.validate();

		// Assert
		let errors_zero = result_zero.unwrap_err();
		assert_eq!(errors_zero.len(), 1);
		assert_eq!(
			errors_zero[0].message,
			"health.port must be between 1 and 65535"
		);
		let errors_over = result_over.unwrap_err();
		assert_eq!(errors_over.len(), 1);
		assert_eq!(
			errors_over[0].message,
			"health.port must be between 1 and 65535"
		);
		let errors_negative = result_negative.unwrap_err();
		assert_eq!(errors_negative.len(), 1);
		assert_eq!(
			errors_negative[0].message,
			"health.port must be between 1 and 65535"
		);
	}

	#[rstest]
	fn health_spec_validation_zero_interval() {
		// Arrange
		let spec = HealthSpec {
			path: None,
			port: None,
			interval_seconds: Some(0),
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.unwrap_err();
		assert_eq!(errors.len(), 1);
		assert_eq!(errors[0].message, "health.interval_seconds must be > 0");
	}

	#[rstest]
	fn services_spec_validation_invalid_ports() {
		// Arrange
		let spec = ServicesSpec {
			port: Some(0),
			target_port: Some(65536),
			ingress_host: None,
			tls: None,
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.unwrap_err();
		assert_eq!(errors.len(), 2);
		assert_eq!(
			errors[0].message,
			"services.port must be between 1 and 65535"
		);
		assert_eq!(
			errors[1].message,
			"services.target_port must be between 1 and 65535"
		);
	}

	#[rstest]
	fn services_tls_validation_requires_ingress_host() {
		// Arrange
		let spec = ProjectSpec {
			image: "img:v1".to_string(),
			services: Some(ServicesSpec {
				port: Some(80),
				target_port: Some(8000),
				ingress_host: None,
				tls: Some(ServiceTlsSpec {
					enabled: true,
					secret_name: Some("app-tls".to_string()),
					issuer: None,
					cluster_issuer: None,
				}),
			}),
			..Default::default()
		};

		// Act
		let errors = spec.validate().expect_err("missing host should fail");

		// Assert
		assert_eq!(errors.len(), 1);
		assert_eq!(
			errors[0].message,
			"services.tls.enabled requires services.ingress_host"
		);
	}

	#[rstest]
	fn services_tls_validation_rejects_blank_ingress_host() {
		// Arrange
		let spec = ProjectSpec {
			image: "img:v1".to_string(),
			services: Some(ServicesSpec {
				port: Some(80),
				target_port: Some(8000),
				ingress_host: Some("   ".to_string()),
				tls: Some(ServiceTlsSpec {
					enabled: true,
					secret_name: Some("app-tls".to_string()),
					issuer: None,
					cluster_issuer: None,
				}),
			}),
			..Default::default()
		};

		// Act
		let errors = spec.validate().expect_err("blank host should fail");

		// Assert
		assert_eq!(errors.len(), 1);
		assert_eq!(
			errors[0].message,
			"services.tls.enabled requires services.ingress_host"
		);
	}

	#[rstest]
	fn services_tls_validation_requires_secret_name() {
		// Arrange
		let spec = ProjectSpec {
			image: "img:v1".to_string(),
			services: Some(ServicesSpec {
				port: Some(80),
				target_port: Some(8000),
				ingress_host: Some("app.example.com".to_string()),
				tls: Some(ServiceTlsSpec {
					enabled: true,
					secret_name: None,
					issuer: None,
					cluster_issuer: None,
				}),
			}),
			..Default::default()
		};

		// Act
		let errors = spec.validate().expect_err("missing secret should fail");

		// Assert
		assert_eq!(errors.len(), 1);
		assert_eq!(
			errors[0].message,
			"services.tls.secret_name is required when services.tls.enabled is true"
		);
	}

	#[rstest]
	fn services_tls_validation_rejects_cluster_issuer() {
		// Arrange
		let spec = ProjectSpec {
			image: "img:v1".to_string(),
			services: Some(ServicesSpec {
				port: Some(80),
				target_port: Some(8000),
				ingress_host: Some("app.example.com".to_string()),
				tls: Some(ServiceTlsSpec {
					enabled: true,
					secret_name: Some("app-tls".to_string()),
					issuer: None,
					cluster_issuer: Some("letsencrypt-prod".to_string()),
				}),
			}),
			..Default::default()
		};

		// Act
		let errors = spec.validate().expect_err("cluster issuers should fail");

		// Assert
		assert_eq!(errors.len(), 1);
		assert_eq!(
			errors[0].message,
			"services.tls.cluster_issuer is not supported; use services.tls.issuer with a namespace-scoped Issuer"
		);
	}

	#[rstest]
	fn project_spec_validation_collects_all_errors() {
		// Arrange
		let spec = ProjectSpec {
			image: "myapp:latest".to_string(),
			replicas: Some(-1),
			scale: Some(ScaleSpec {
				min_replicas: Some(-1),
				max_replicas: Some(-2),
				metric: None,
				target_value: Some(0),
			}),
			health: Some(HealthSpec {
				path: None,
				port: Some(0),
				interval_seconds: Some(0),
			}),
			services: Some(ServicesSpec {
				port: Some(0),
				target_port: Some(65536),
				ingress_host: None,
				tls: None,
			}),
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.unwrap_err();
		// replicas(-1) + min(-1) + max(-2) + max<min + target(0) + health.port(0) + interval(0) + services.port(0) + services.target_port(65536)
		assert_eq!(errors.len(), 9);
		assert_eq!(errors[0].message, "spec.replicas must be >= 0");
		assert_eq!(errors[1].message, "scale.min_replicas must be >= 1");
		assert_eq!(errors[2].message, "scale.max_replicas must be >= 1");
		assert_eq!(
			errors[3].message,
			"scale.max_replicas must be >= scale.min_replicas"
		);
		assert_eq!(errors[4].message, "scale.target_value must be > 0");
		assert_eq!(errors[5].message, "health.port must be between 1 and 65535");
		assert_eq!(errors[6].message, "health.interval_seconds must be > 0");
		assert_eq!(
			errors[7].message,
			"services.port must be between 1 and 65535"
		);
		assert_eq!(
			errors[8].message,
			"services.target_port must be between 1 and 65535"
		);
	}

	#[rstest]
	fn test_spec_with_database_spec_serializes() {
		// Arrange
		let spec = ProjectSpec {
			image: "myapp:v1".to_string(),
			replicas: Some(2),
			database: Some(DatabaseSpec {
				engine: DatabaseEngine::Postgresql,
				instance_class: None,
				storage_gb: Some(20),
				version: None,
			}),
			..Default::default()
		};

		// Act
		let json = serde_json::to_string(&spec).unwrap();
		let parsed: ProjectSpec = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(parsed.database.unwrap().engine, DatabaseEngine::Postgresql);
	}

	#[rstest]
	fn test_spec_validate_rejects_invalid_nested_database() {
		// Arrange
		let spec = ProjectSpec {
			image: "myapp:v1".to_string(),
			database: Some(DatabaseSpec {
				engine: DatabaseEngine::Postgresql,
				instance_class: None,
				storage_gb: Some(-1),
				version: None,
			}),
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		assert!(result.is_err());
		let errors = result.unwrap_err();
		assert_eq!(errors.len(), 1);
		assert_eq!(errors[0].message, "database.storage_gb must be > 0");
	}

	#[rstest]
	fn test_spec_validate_rejects_invalid_nested_worker() {
		// Arrange
		let spec = ProjectSpec {
			image: "myapp:v1".to_string(),
			worker: Some(WorkerSpec {
				concurrency: Some(0),
				command: None,
			}),
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		assert!(result.is_err());
		let errors = result.unwrap_err();
		assert_eq!(errors.len(), 1);
		assert_eq!(errors[0].message, "worker.concurrency must be > 0");
	}

	#[rstest]
	fn test_spec_validate_rejects_invalid_nested_mail() {
		// Arrange
		let spec = ProjectSpec {
			image: "myapp:v1".to_string(),
			mail: Some(MailSpec {
				smtp_host: None,
				smtp_port: Some(0),
				credentials_secret: None,
			}),
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		assert!(result.is_err());
		let errors = result.unwrap_err();
		assert_eq!(errors.len(), 1);
		assert_eq!(
			errors[0].message,
			"mail.smtp_port must be between 1 and 65535"
		);
	}

	#[rstest]
	fn test_spec_deletion_policy_defaults_to_retain() {
		// Arrange
		let json = r#"{"image": "myapp:v1"}"#;

		// Act
		let spec: ProjectSpec = serde_json::from_str(json).unwrap();

		// Assert
		assert_eq!(spec.deletion_policy, DeletionPolicy::Retain);
	}

	#[rstest]
	fn test_spec_validate_rejects_invalid_nested_mail_port_99999() {
		// Arrange
		let spec = ProjectSpec {
			image: "myapp:v1".to_string(),
			mail: Some(MailSpec {
				smtp_host: None,
				smtp_port: Some(99999),
				credentials_secret: None,
			}),
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		assert!(result.is_err());
		let errors = result.unwrap_err();
		assert_eq!(errors.len(), 1);
		assert_eq!(
			errors[0].message,
			"mail.smtp_port must be between 1 and 65535"
		);
	}

	#[rstest]
	fn test_spec_with_all_optional_fields_set() {
		// Arrange
		use crate::crd::auth::AuthSpec;
		use crate::crd::cache::{CacheBackend, CacheSpec};
		use crate::crd::storage::{StorageBackend, StorageSpec};
		let spec = ProjectSpec {
			image: "full-app:v3".to_string(),
			replicas: Some(5),
			database: Some(DatabaseSpec {
				engine: DatabaseEngine::Postgresql,
				instance_class: Some("db.t3.large".to_string()),
				storage_gb: Some(100),
				version: Some("16".to_string()),
			}),
			cache: Some(CacheSpec {
				backend: CacheBackend::Redis,
				instance_type: Some("cache.t3.micro".to_string()),
			}),
			worker: Some(WorkerSpec {
				concurrency: Some(8),
				command: Some(vec!["worker".to_string(), "start".to_string()]),
			}),
			auth: Some(AuthSpec {
				jwt: true,
				oauth: Some(crate::crd::auth::OAuthSpec {
					provider: "google".to_string(),
					credentials_secret: Some("oauth-secret".to_string()),
				}),
			}),
			storage: Some(StorageSpec {
				backend: Some(StorageBackend::S3),
				bucket: Some("my-bucket".to_string()),
			}),
			mail: Some(MailSpec {
				smtp_host: Some("smtp.example.com".to_string()),
				smtp_port: Some(587),
				credentials_secret: Some("mail-secret".to_string()),
			}),
			scale: Some(ScaleSpec {
				min_replicas: Some(2),
				max_replicas: Some(20),
				metric: Some(ScaleMetric::Rps),
				target_value: Some(1000),
			}),
			health: Some(HealthSpec {
				path: Some("/healthz".to_string()),
				port: Some(8080),
				interval_seconds: Some(10),
			}),
			services: Some(ServicesSpec {
				port: Some(443),
				target_port: Some(8080),
				ingress_host: Some("app.example.com".to_string()),
				tls: None,
			}),
			pages: Some(crate::crd::pages::PagesSpec {
				static_root: Some("/app/dist".to_string()),
				static_url: Some("/static/".to_string()),
				server_image: None,
				server_resources: None,
				cache_max_age: Some(86400),
				brotli: None,
				gzip: None,
			}),
			deletion_policy: DeletionPolicy::Delete,
			features: vec!["db-postgres".to_string(), "auth-jwt".to_string()],
			env: BTreeMap::from([("MY_VAR".to_string(), "my_val".to_string())]),
			introspect: None,
			isolation: None,
			source: None,
			plugins: None,
			image_pull_secrets: None,
			service_account: None,
			infrastructure: None,
			tenant: None,
		};

		// Act
		let result = spec.validate();

		// Assert
		assert!(result.is_ok());
		assert_eq!(spec.features.len(), 2);
		assert_eq!(spec.env.len(), 1);
		assert_eq!(spec.deletion_policy, DeletionPolicy::Delete);
		assert!(spec.pages.is_some());
	}

	#[rstest]
	fn test_spec_with_features_list_populated() {
		// Arrange
		let json = r#"{
			"image": "myapp:v1",
			"features": ["db-postgres", "auth-jwt", "sessions"]
		}"#;

		// Act
		let spec: ProjectSpec = serde_json::from_str(json).unwrap();

		// Assert
		assert_eq!(spec.features.len(), 3);
		assert_eq!(spec.features[0], "db-postgres");
		assert_eq!(spec.features[1], "auth-jwt");
		assert_eq!(spec.features[2], "sessions");
	}

	#[rstest]
	fn test_spec_features_defaults_to_empty() {
		// Arrange
		let json = r#"{"image": "myapp:v1"}"#;

		// Act
		let spec: ProjectSpec = serde_json::from_str(json).unwrap();

		// Assert
		assert!(spec.features.is_empty());
	}

	#[rstest]
	fn test_project_spec_with_introspect() {
		// Arrange
		use crate::introspect::{AppMetadata, IntrospectOutput};
		let spec = ProjectSpec {
			image: "myapp:latest".to_string(),
			introspect: Some(IntrospectOutput {
				app: AppMetadata {
					name: "my-app".to_string(),
					version: "1.0.0".to_string(),
				},
				..Default::default()
			}),
			..Default::default()
		};

		// Act
		let json = serde_json::to_string(&spec).expect("serialization should succeed");
		let value: serde_json::Value = serde_json::from_str(&json).expect("parsing should succeed");

		// Assert
		assert_eq!(value["image"], "myapp:latest");
		assert_eq!(value["introspect"]["app"]["name"], "my-app");
		assert_eq!(value["introspect"]["app"]["version"], "1.0.0");
	}

	#[rstest]
	fn test_spec_with_pages_roundtrip() {
		// Arrange
		let spec = ProjectSpec {
			image: "app:v1".to_string(),
			pages: Some(crate::crd::pages::PagesSpec {
				static_root: Some("/app/dist".to_string()),
				static_url: Some("/static/".to_string()),
				server_image: None,
				server_resources: None,
				cache_max_age: Some(86400),
				brotli: None,
				gzip: None,
			}),
			..Default::default()
		};

		// Act
		let yaml = serde_yaml::to_string(&spec).unwrap();
		let deserialized: ProjectSpec = serde_yaml::from_str(&yaml).unwrap();

		// Assert
		assert!(deserialized.pages.is_some());
		let pages = deserialized.pages.unwrap();
		assert_eq!(pages.static_root.unwrap(), "/app/dist");
		assert_eq!(pages.cache_max_age.unwrap(), 86400);
	}

	#[rstest]
	fn test_spec_pages_validation_delegated() {
		// Arrange
		let spec = ProjectSpec {
			image: "app:v1".to_string(),
			pages: Some(crate::crd::pages::PagesSpec {
				static_root: Some(String::new()),
				static_url: None,
				server_image: None,
				server_resources: None,
				cache_max_age: None,
				brotli: None,
				gzip: None,
			}),
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.unwrap_err();
		assert!(errors.iter().any(|e| e.message.contains("static_root")));
	}

	#[rstest]
	fn test_project_spec_backward_compatible() {
		// Arrange: JSON without introspect field (pre-existing format)
		let json = r#"{"image": "legacy-app:v2", "replicas": 3}"#;

		// Act
		let spec: ProjectSpec = serde_json::from_str(json).expect("deserialization should succeed");

		// Assert
		assert_eq!(spec.image, "legacy-app:v2");
		assert_eq!(spec.replicas, Some(3));
		assert!(spec.introspect.is_none());
	}

	#[rstest]
	fn test_spec_isolation_field_backward_compatible() {
		// Arrange: JSON without isolation field (pre-existing format)
		let json = r#"{"image": "legacy-app:v2", "replicas": 3}"#;

		// Act
		let spec: ProjectSpec = serde_json::from_str(json).expect("deserialization should succeed");

		// Assert
		assert_eq!(spec.image, "legacy-app:v2");
		assert!(spec.isolation.is_none());
	}

	#[rstest]
	fn test_spec_with_isolation_microvm() {
		// Arrange
		use crate::crd::isolation::IsolationLevel;
		let json = r#"{
			"image": "myapp:v1",
			"isolation": {
				"level": "MicroVM",
				"network": {
					"block_metadata_service": true,
					"egress_allow_cidrs": ["10.0.0.0/8"]
				}
			}
		}"#;

		// Act
		let spec: ProjectSpec = serde_json::from_str(json).expect("deserialization should succeed");

		// Assert
		let isolation = spec.isolation.unwrap();
		assert_eq!(isolation.level, IsolationLevel::MicroVM);
		assert!(isolation.network.unwrap().block_metadata_service);
	}

	#[rstest]
	fn test_spec_validate_rejects_invalid_isolation() {
		// Arrange
		let spec = ProjectSpec {
			image: "myapp:v1".to_string(),
			isolation: Some(IsolationSpec {
				runtime_class_override: Some(String::new()),
				..Default::default()
			}),
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.unwrap_err();
		assert!(errors.iter().any(|e| e.message.contains("non-empty")));
	}

	#[rstest]
	fn test_spec_isolation_skipped_in_serialization_when_none() {
		// Arrange
		let spec = ProjectSpec {
			image: "myapp:v1".to_string(),
			..Default::default()
		};

		// Act
		let json = serde_json::to_string(&spec).unwrap();
		let value: serde_json::Value = serde_json::from_str(&json).unwrap();

		// Assert
		assert!(value.get("isolation").is_none());
	}

	#[rstest]
	fn test_spec_source_field_backward_compatible() {
		// Arrange: JSON without source field (pre-existing format)
		let json = r#"{"image": "legacy-app:v2", "replicas": 3}"#;

		// Act
		let spec: ProjectSpec = serde_json::from_str(json).expect("deserialization should succeed");

		// Assert
		assert_eq!(spec.image, "legacy-app:v2");
		assert!(spec.source.is_none());
	}

	#[rstest]
	fn test_spec_source_skipped_in_serialization_when_none() {
		// Arrange
		let spec = ProjectSpec {
			image: "myapp:v1".to_string(),
			..Default::default()
		};

		// Act
		let json = serde_json::to_string(&spec).unwrap();
		let value: serde_json::Value = serde_json::from_str(&json).unwrap();

		// Assert
		assert!(value.get("source").is_none());
	}

	#[rstest]
	fn test_spec_with_source_roundtrip() {
		// Arrange
		use crate::crd::source::{
			BuildSpec, GitProvider, PreviewOverrides, PreviewSpec, SourceSpec, WebhookEvent,
			WebhookSpec,
		};
		let spec = ProjectSpec {
			image: "app:v1".to_string(),
			source: Some(SourceSpec {
				repository: "https://github.com/myorg/myapp".to_string(),
				branch: Some("main".to_string()),
				provider: Some(GitProvider::GitHub),
				credentials_secret: Some("git-token".to_string()),
				build: Some(BuildSpec {
					dockerfile: Some("Dockerfile".to_string()),
					context: Some(".".to_string()),
					registry: Some("ghcr.io/myorg".to_string()),
					build_args: std::collections::BTreeMap::from([(
						"MODE".to_string(),
						"release".to_string(),
					)]),
				}),
				webhook: Some(WebhookSpec {
					enabled: true,
					events: vec![WebhookEvent::Push],
					secret_ref: Some("wh-secret".to_string()),
				}),
				preview: Some(PreviewSpec {
					enabled: true,
					ttl: Some("72h".to_string()),
					url_template: Some("{{branch}}.preview.example.com".to_string()),
					overrides: Some(PreviewOverrides {
						replicas: Some(1),
						database: Some(false),
						cache: Some(false),
					}),
					budget: None,
				}),
			}),
			..Default::default()
		};

		// Act
		let yaml = serde_yaml::to_string(&spec).unwrap();
		let deserialized: ProjectSpec = serde_yaml::from_str(&yaml).unwrap();

		// Assert
		assert!(deserialized.source.is_some());
		let source = deserialized.source.unwrap();
		assert_eq!(source.repository, "https://github.com/myorg/myapp");
		assert_eq!(source.branch.unwrap(), "main");
		assert_eq!(source.provider.unwrap(), GitProvider::GitHub);
		assert!(source.build.is_some());
		assert!(source.webhook.is_some());
		assert!(source.preview.is_some());
	}

	#[rstest]
	fn test_spec_validate_rejects_invalid_source() {
		// Arrange
		use crate::crd::source::SourceSpec;
		let spec = ProjectSpec {
			image: "myapp:v1".to_string(),
			source: Some(SourceSpec {
				repository: String::new(),
				branch: None,
				provider: None,
				credentials_secret: None,
				build: None,
				webhook: None,
				preview: None,
			}),
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.unwrap_err();
		assert!(errors.iter().any(|e| e.message.contains("repository")));
	}

	#[rstest]
	fn test_spec_plugins_field_backward_compatible() {
		// Arrange: JSON without plugins field (pre-existing format)
		let json = r#"{"image": "legacy-app:v2", "replicas": 3}"#;

		// Act
		let spec: ProjectSpec = serde_json::from_str(json).expect("deserialization should succeed");

		// Assert
		assert_eq!(spec.image, "legacy-app:v2");
		assert!(spec.plugins.is_none());
	}

	#[rstest]
	fn test_spec_plugins_skipped_in_serialization_when_none() {
		// Arrange
		let spec = ProjectSpec {
			image: "myapp:v1".to_string(),
			..Default::default()
		};

		// Act
		let json = serde_json::to_string(&spec).unwrap();
		let value: serde_json::Value = serde_json::from_str(&json).unwrap();

		// Assert
		assert!(value.get("plugins").is_none());
	}

	#[rstest]
	fn test_spec_with_plugins_roundtrip() {
		// Arrange
		use crate::crd::plugins::{PluginCapability, PluginSpec, PluginType};
		let spec = ProjectSpec {
			image: "app:v1".to_string(),
			plugins: Some(vec![PluginSpec {
				name: "auth-gate".to_string(),
				wasm_dir: "/var/lib/dentdelion/auth-gate".to_string(),
				plugin_type: PluginType::HttpMiddleware,
				memory_limit_mb: Some(64),
				timeout_ms: Some(500),
				capabilities: vec![PluginCapability::NetworkAccess],
			}]),
			..Default::default()
		};

		// Act
		let yaml = serde_yaml::to_string(&spec).unwrap();
		let parsed: ProjectSpec = serde_yaml::from_str(&yaml).unwrap();

		// Assert
		let plugins = parsed.plugins.unwrap();
		assert_eq!(plugins.len(), 1);
		assert_eq!(plugins[0].name, "auth-gate");
		assert_eq!(plugins[0].plugin_type, PluginType::HttpMiddleware);
		assert_eq!(plugins[0].capabilities.len(), 1);
	}

	#[rstest]
	fn test_spec_validate_rejects_invalid_plugin_entry() {
		// Arrange
		use crate::crd::plugins::{PluginSpec, PluginType};
		let spec = ProjectSpec {
			image: "app:v1".to_string(),
			plugins: Some(vec![PluginSpec {
				name: String::new(),
				wasm_dir: "/var/lib/dentdelion/p".to_string(),
				plugin_type: PluginType::HttpMiddleware,
				memory_limit_mb: None,
				timeout_ms: None,
				capabilities: Vec::new(),
			}]),
			..Default::default()
		};

		// Act
		let errors = spec.validate().expect_err("validation should fail");

		// Assert
		assert!(errors.iter().any(|e| e.message.contains("plugins[].name")));
	}

	#[rstest]
	fn test_spec_validate_rejects_duplicate_plugin_volume_suffix() {
		// Arrange — names sanitize to the same Volume suffix.
		use crate::crd::plugins::{PluginSpec, PluginType};
		let spec = ProjectSpec {
			image: "app:v1".to_string(),
			plugins: Some(vec![
				PluginSpec {
					name: "my.plugin".to_string(),
					wasm_dir: "/var/lib/dentdelion/a".to_string(),
					plugin_type: PluginType::HttpMiddleware,
					memory_limit_mb: None,
					timeout_ms: None,
					capabilities: Vec::new(),
				},
				PluginSpec {
					name: "my-plugin".to_string(),
					wasm_dir: "/var/lib/dentdelion/b".to_string(),
					plugin_type: PluginType::HttpMiddleware,
					memory_limit_mb: None,
					timeout_ms: None,
					capabilities: Vec::new(),
				},
			]),
			..Default::default()
		};

		// Act
		let errors = spec
			.validate()
			.expect_err("duplicate sanitized plugin name should be rejected");

		// Assert
		assert!(
			errors
				.iter()
				.any(|e| e.message.contains("collides on Volume suffix 'my-plugin'"))
		);
	}

	#[rstest]
	fn test_spec_validate_rejects_duplicate_plugin_wasm_dir() {
		// Arrange
		use crate::crd::plugins::{PluginSpec, PluginType};
		let spec = ProjectSpec {
			image: "app:v1".to_string(),
			plugins: Some(vec![
				PluginSpec {
					name: "alpha".to_string(),
					wasm_dir: "/var/lib/dentdelion/shared".to_string(),
					plugin_type: PluginType::HttpMiddleware,
					memory_limit_mb: None,
					timeout_ms: None,
					capabilities: Vec::new(),
				},
				PluginSpec {
					name: "beta".to_string(),
					wasm_dir: "/var/lib/dentdelion/shared".to_string(),
					plugin_type: PluginType::HttpMiddleware,
					memory_limit_mb: None,
					timeout_ms: None,
					capabilities: Vec::new(),
				},
			]),
			..Default::default()
		};

		// Act
		let errors = spec
			.validate()
			.expect_err("duplicate wasm_dir should be rejected");

		// Assert
		assert!(errors.iter().any(|e| {
			e.message
				.contains("duplicate wasm_dir '/var/lib/dentdelion/shared'")
		}));
	}

	#[rstest]
	fn test_spec_validate_rejects_overlapping_plugin_wasm_dir() {
		// Arrange
		use crate::crd::plugins::{PluginSpec, PluginType};
		let spec = ProjectSpec {
			image: "app:v1".to_string(),
			plugins: Some(vec![
				PluginSpec {
					name: "alpha".to_string(),
					wasm_dir: "/var/lib/dentdelion/plugins".to_string(),
					plugin_type: PluginType::HttpMiddleware,
					memory_limit_mb: None,
					timeout_ms: None,
					capabilities: Vec::new(),
				},
				PluginSpec {
					name: "beta".to_string(),
					wasm_dir: "/var/lib/dentdelion/plugins/beta".to_string(),
					plugin_type: PluginType::HttpMiddleware,
					memory_limit_mb: None,
					timeout_ms: None,
					capabilities: Vec::new(),
				},
			]),
			..Default::default()
		};

		// Act
		let errors = spec
			.validate()
			.expect_err("overlapping wasm_dir values should be rejected");

		// Assert
		assert!(errors.iter().any(|e| {
			e.message.contains(
				"overlapping wasm_dir mount paths '/var/lib/dentdelion/plugins' and '/var/lib/dentdelion/plugins/beta'",
			)
		}));
	}

	#[rstest]
	fn test_spec_service_account_field_backward_compatible() {
		// Arrange: JSON without serviceAccount field (pre-existing format)
		let json = r#"{"image": "legacy-app:v2", "replicas": 3}"#;

		// Act
		let spec: ProjectSpec = serde_json::from_str(json).expect("deserialization should succeed");

		// Assert
		assert_eq!(spec.image, "legacy-app:v2");
		assert!(spec.service_account.is_none());
	}

	#[rstest]
	fn test_spec_service_account_skipped_in_serialization_when_none() {
		// Arrange
		let spec = ProjectSpec {
			image: "myapp:v1".to_string(),
			..Default::default()
		};

		// Act
		let json = serde_json::to_string(&spec).unwrap();
		let value: serde_json::Value = serde_json::from_str(&json).unwrap();

		// Assert
		assert!(value.get("serviceAccount").is_none());
	}

	#[rstest]
	fn test_spec_with_service_account_roundtrip() {
		// Arrange
		let spec = ProjectSpec {
			image: "app:v1".to_string(),
			service_account: Some(ServiceAccountSpec {
				create: true,
				name: Some("my-app".to_string()),
				annotations: BTreeMap::from([
					(
						"iam.gke.io/gcp-service-account".to_string(),
						"my-app@project.iam.gserviceaccount.com".to_string(),
					),
					(
						"eks.amazonaws.com/role-arn".to_string(),
						"arn:aws:iam::123456789012:role/my-app".to_string(),
					),
				]),
			}),
			..Default::default()
		};

		// Act
		let yaml = serde_yaml::to_string(&spec).unwrap();
		let deserialized: ProjectSpec = serde_yaml::from_str(&yaml).unwrap();

		// Assert
		let sa = deserialized
			.service_account
			.expect("serviceAccount should roundtrip");
		assert!(sa.create);
		assert_eq!(sa.name.as_deref(), Some("my-app"));
		assert_eq!(sa.annotations.len(), 2);
		assert_eq!(
			sa.annotations
				.get("iam.gke.io/gcp-service-account")
				.map(String::as_str),
			Some("my-app@project.iam.gserviceaccount.com"),
		);
	}

	#[rstest]
	fn test_spec_validate_rejects_invalid_service_account() {
		// Arrange
		let spec = ProjectSpec {
			image: "myapp:v1".to_string(),
			service_account: Some(ServiceAccountSpec {
				create: true,
				name: Some(String::new()),
				..Default::default()
			}),
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.expect_err("invalid service_account should fail validation");
		assert!(errors.iter().any(|e| e.message.contains("non-empty")));
	}

	#[rstest]
	fn test_spec_image_pull_secrets_field_backward_compatible() {
		// Arrange: JSON without imagePullSecrets (pre-existing format).
		let json = r#"{"image": "legacy-app:v2", "replicas": 3}"#;

		// Act
		let spec: ProjectSpec = serde_json::from_str(json).expect("deserialization should succeed");

		// Assert
		assert_eq!(spec.image, "legacy-app:v2");
		assert!(spec.image_pull_secrets.is_none());
	}

	#[rstest]
	fn test_spec_image_pull_secrets_skipped_in_serialization_when_none() {
		// Arrange
		let spec = ProjectSpec {
			image: "myapp:v1".to_string(),
			..Default::default()
		};

		// Act
		let json = serde_json::to_string(&spec).unwrap();
		let value: serde_json::Value = serde_json::from_str(&json).unwrap();

		// Assert
		assert!(value.get("imagePullSecrets").is_none());
	}

	#[rstest]
	fn test_spec_image_pull_secrets_roundtrip_single_secret() {
		// Arrange
		let spec = ProjectSpec {
			image: "private-registry.example.com/app:v1".to_string(),
			image_pull_secrets: Some(vec![LocalObjectReference {
				name: "regcred".to_string(),
			}]),
			..Default::default()
		};

		// Act
		let json = serde_json::to_string(&spec).expect("serialization should succeed");
		let deserialized: ProjectSpec =
			serde_json::from_str(&json).expect("deserialization should succeed");

		// Assert
		let secrets = deserialized
			.image_pull_secrets
			.expect("image_pull_secrets should be set");
		assert_eq!(secrets.len(), 1);
		assert_eq!(secrets[0].name, "regcred");
		// The serialized form must use the camelCase wire name.
		let value: serde_json::Value = serde_json::from_str(&json).unwrap();
		assert!(value.get("imagePullSecrets").is_some());
	}

	#[rstest]
	fn test_spec_image_pull_secrets_roundtrip_multiple_secrets() {
		// Arrange
		let spec = ProjectSpec {
			image: "private-registry.example.com/app:v1".to_string(),
			image_pull_secrets: Some(vec![
				LocalObjectReference {
					name: "ghcr-pull".to_string(),
				},
				LocalObjectReference {
					name: "ecr-pull".to_string(),
				},
				LocalObjectReference {
					name: "gar-pull".to_string(),
				},
			]),
			..Default::default()
		};

		// Act
		let yaml = serde_yaml::to_string(&spec).expect("serialization should succeed");
		let deserialized: ProjectSpec =
			serde_yaml::from_str(&yaml).expect("deserialization should succeed");

		// Assert
		let secrets = deserialized
			.image_pull_secrets
			.expect("image_pull_secrets should be set");
		assert_eq!(secrets.len(), 3);
		assert_eq!(secrets[0].name, "ghcr-pull");
		assert_eq!(secrets[1].name, "ecr-pull");
		assert_eq!(secrets[2].name, "gar-pull");
	}

	#[rstest]
	fn test_spec_image_pull_secrets_default_is_none() {
		// Arrange & Act
		let spec = ProjectSpec::default();

		// Assert
		assert!(spec.image_pull_secrets.is_none());
	}

	#[rstest]
	fn test_spec_with_source_from_json() {
		// Arrange
		let json = r#"{
			"image": "myapp:v1",
			"source": {
				"repository": "https://github.com/myorg/myapp",
				"branch": "develop",
				"provider": "gitlab"
			}
		}"#;

		// Act
		let spec: ProjectSpec = serde_json::from_str(json).expect("deserialization should succeed");

		// Assert
		let source = spec.source.unwrap();
		assert_eq!(source.repository, "https://github.com/myorg/myapp");
		assert_eq!(source.branch.unwrap(), "develop");
		assert_eq!(
			source.provider.unwrap(),
			crate::crd::source::GitProvider::GitLab
		);
	}

	#[rstest]
	fn project_spec_validation_accepts_valid_tenant_org_only() {
		// Arrange
		let spec = ProjectSpec {
			image: "myapp:latest".to_string(),
			tenant: Some(TenantRef {
				organization: "acme-prod".to_string(),
				team: None,
			}),
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		assert!(result.is_ok(), "expected valid tenant to pass: {result:?}");
	}

	#[rstest]
	fn project_spec_validation_accepts_valid_tenant_with_team() {
		// Arrange
		let spec = ProjectSpec {
			image: "myapp:latest".to_string(),
			tenant: Some(TenantRef {
				organization: "acme".to_string(),
				team: Some("platform".to_string()),
			}),
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		assert!(result.is_ok(), "expected valid tenant to pass: {result:?}");
	}

	#[rstest]
	fn project_spec_validation_skips_when_tenant_absent() {
		// Arrange — backward-compatibility: CRs without tenant must continue to pass.
		let spec = ProjectSpec {
			image: "myapp:latest".to_string(),
			tenant: None,
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		assert!(result.is_ok());
	}

	#[rstest]
	#[case("ACME", "spec.tenant.organization")]
	#[case("acme_prod", "spec.tenant.organization")]
	#[case("", "spec.tenant.organization")]
	fn project_spec_validation_rejects_invalid_tenant_organization(
		#[case] organization: &str,
		#[case] expected_prefix: &str,
	) {
		// Arrange
		let spec = ProjectSpec {
			image: "myapp:latest".to_string(),
			tenant: Some(TenantRef {
				organization: organization.to_string(),
				team: None,
			}),
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.expect_err("expected tenant validation to fail");
		assert!(
			errors[0].message.starts_with(expected_prefix),
			"expected message prefix {expected_prefix:?}, got {:?}",
			errors[0].message
		);
	}

	#[rstest]
	#[case("BAD")]
	#[case("team_underscore")]
	fn project_spec_validation_rejects_invalid_tenant_team(#[case] team: &str) {
		// Arrange
		let spec = ProjectSpec {
			image: "myapp:latest".to_string(),
			tenant: Some(TenantRef {
				organization: "acme".to_string(),
				team: Some(team.to_string()),
			}),
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.expect_err("expected tenant validation to fail");
		assert!(
			errors
				.iter()
				.any(|e| e.message.starts_with("spec.tenant.team")),
			"expected spec.tenant.team error, got {errors:?}"
		);
	}

	#[rstest]
	fn project_spec_serializes_tenant_field() {
		// Arrange
		let spec = ProjectSpec {
			image: "myapp:v1".to_string(),
			tenant: Some(TenantRef {
				organization: "acme".to_string(),
				team: Some("platform".to_string()),
			}),
			..Default::default()
		};

		// Act
		let json = serde_json::to_value(&spec).expect("serialization should succeed");

		// Assert
		assert_eq!(json["tenant"]["organization"], "acme");
		assert_eq!(json["tenant"]["team"], "platform");
	}

	#[rstest]
	fn project_spec_omits_tenant_when_none() {
		// Arrange
		let spec = ProjectSpec {
			image: "myapp:v1".to_string(),
			tenant: None,
			..Default::default()
		};

		// Act
		let json = serde_json::to_value(&spec).expect("serialization should succeed");

		// Assert — skip_serializing_if = "Option::is_none" means the field is omitted entirely.
		assert!(
			json.get("tenant").is_none(),
			"tenant should be omitted, got {json:?}"
		);
	}
}
