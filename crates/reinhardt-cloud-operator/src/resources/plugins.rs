//! Builders for dentdelion WASM plugin resources.
//!
//! Materializes `spec.plugins` on a `ReinhardtApp` into:
//!
//! - a `ConfigMap` carrying a serialized `dentdelion.toml` document
//!   with one `[[plugins]]` entry per [`PluginSpec`]
//! - a set of `Volume` + `VolumeMount` pairs that:
//!     * mount the `dentdelion.toml` `ConfigMap` at a well-known path
//!     * expose an empty directory per plugin at the declared `wasm_dir`
//!
//! The WASM artifact itself is expected to be delivered into the plugin
//! volume by a separate mechanism (e.g. an init container or an image
//! with the artifact baked in). This module only provisions the mount
//! points and the declarative configuration consumed by the application
//! at startup.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{
	ConfigMap, ConfigMapVolumeSource, EmptyDirVolumeSource, Volume, VolumeMount,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::ResourceExt;
use reinhardt_cloud_types::crd::{PluginSpec, ReinhardtApp};
use serde::Serialize;

use super::labels::{Component, owner_reference, standard_labels};
use crate::error::Error;

/// File name for the rendered dentdelion plugin configuration document.
pub(crate) const DENTDELION_CONFIG_FILE: &str = "dentdelion.toml";

/// Mount path inside the application container for the plugin config `ConfigMap`.
pub(crate) const DENTDELION_CONFIG_MOUNT_DIR: &str = "/etc/dentdelion";

/// Name of the Volume that carries the rendered `dentdelion.toml` `ConfigMap`.
const PLUGIN_CONFIG_VOLUME_NAME: &str = "dentdelion-config";

/// Returns the `ConfigMap` name used for the plugin configuration document.
pub(crate) fn plugin_configmap_name(app: &ReinhardtApp) -> String {
	format!("{}-dentdelion-plugins", app.name_any())
}

/// Sanitized volume name for an individual plugin's WASM directory.
///
/// Delegates name sanitization to [`sanitized_volume_suffix`] so that
/// validation in `ReinhardtAppSpec::validate` and materialization here
/// share a single source of truth.
fn plugin_volume_name(plugin: &PluginSpec) -> String {
	format!(
		"dentdelion-{}",
		reinhardt_cloud_types::crd::plugins::sanitized_volume_suffix(&plugin.name)
	)
}

/// Serializable view of a [`PluginSpec`] for the dentdelion TOML document.
#[derive(Debug, Serialize)]
struct PluginTomlEntry<'a> {
	name: &'a str,
	#[serde(rename = "type")]
	plugin_type: &'a str,
	wasm_dir: &'a str,
	#[serde(skip_serializing_if = "Option::is_none")]
	memory_limit_mb: Option<u64>,
	#[serde(skip_serializing_if = "Option::is_none")]
	timeout_ms: Option<u64>,
	capabilities: Vec<&'a str>,
}

/// Top-level shape of the rendered `dentdelion.toml` document.
#[derive(Debug, Serialize)]
struct PluginTomlDocument<'a> {
	#[serde(rename = "plugins")]
	plugins: Vec<PluginTomlEntry<'a>>,
}

/// Returns the TOML string representation for a given plugin type variant.
fn plugin_type_str(ty: &reinhardt_cloud_types::crd::PluginType) -> &'static str {
	use reinhardt_cloud_types::crd::PluginType;
	match ty {
		PluginType::HttpMiddleware => "http-middleware",
		PluginType::GrpcInterceptor => "grpc-interceptor",
		PluginType::EventHandler => "event-handler",
	}
}

/// Returns the TOML string representation for a given capability variant.
fn capability_str(cap: &reinhardt_cloud_types::crd::PluginCapability) -> &'static str {
	use reinhardt_cloud_types::crd::PluginCapability;
	match cap {
		PluginCapability::NetworkAccess => "network",
		PluginCapability::FilesystemRead => "fs-read",
		PluginCapability::FilesystemWrite => "fs-write",
		PluginCapability::EnvAccess => "env",
	}
}

/// Renders `spec.plugins` into the full `dentdelion.toml` document body.
pub(crate) fn render_plugin_config(plugins: &[PluginSpec]) -> Result<String, Error> {
	let entries: Vec<PluginTomlEntry<'_>> = plugins
		.iter()
		.map(|p| PluginTomlEntry {
			name: &p.name,
			plugin_type: plugin_type_str(&p.plugin_type),
			wasm_dir: &p.wasm_dir,
			memory_limit_mb: p.memory_limit_mb,
			timeout_ms: p.timeout_ms,
			capabilities: p.capabilities.iter().map(capability_str).collect(),
		})
		.collect();

	let doc = PluginTomlDocument { plugins: entries };

	toml::to_string(&doc).map_err(|e| Error::PluginConfigRender(e.to_string()))
}

/// Builds a `ConfigMap` carrying the rendered `dentdelion.toml` document.
///
/// Returns `Ok(None)` when the spec declares no plugins; callers should
/// treat that as "no ConfigMap is needed".
pub(crate) fn build_plugin_configmap(app: &ReinhardtApp) -> Result<Option<ConfigMap>, Error> {
	let Some(plugins) = app.spec.plugins.as_ref() else {
		return Ok(None);
	};
	if plugins.is_empty() {
		return Ok(None);
	}

	let labels = standard_labels(app, Component::Plugins);
	let namespace = super::require_namespace(app)?;
	let owner_ref = owner_reference(app)?;
	let rendered = render_plugin_config(plugins)?;

	let data = BTreeMap::from([(DENTDELION_CONFIG_FILE.to_string(), rendered)]);

	Ok(Some(ConfigMap {
		metadata: ObjectMeta {
			name: Some(plugin_configmap_name(app)),
			namespace: Some(namespace),
			labels: Some(labels),
			owner_references: Some(vec![owner_ref]),
			..Default::default()
		},
		data: Some(data),
		..Default::default()
	}))
}

/// Builds the `Volume`s and `VolumeMount`s needed by the application pod
/// to consume the declared plugins.
///
/// The returned vectors are empty when the spec declares no plugins.
pub(crate) fn build_plugin_volumes(app: &ReinhardtApp) -> (Vec<Volume>, Vec<VolumeMount>) {
	let Some(plugins) = app.spec.plugins.as_ref() else {
		return (Vec::new(), Vec::new());
	};
	if plugins.is_empty() {
		return (Vec::new(), Vec::new());
	}

	// One Volume + VolumeMount for the ConfigMap-backed dentdelion.toml,
	// plus one Volume + VolumeMount per plugin for the WASM artifact
	// directory (backed by an emptyDir so the application can populate it
	// at startup from an init container or bundled image).
	let mut volumes: Vec<Volume> = Vec::with_capacity(plugins.len() + 1);
	let mut mounts: Vec<VolumeMount> = Vec::with_capacity(plugins.len() + 1);

	volumes.push(Volume {
		name: PLUGIN_CONFIG_VOLUME_NAME.to_string(),
		config_map: Some(ConfigMapVolumeSource {
			name: plugin_configmap_name(app),
			..Default::default()
		}),
		..Default::default()
	});
	mounts.push(VolumeMount {
		name: PLUGIN_CONFIG_VOLUME_NAME.to_string(),
		mount_path: DENTDELION_CONFIG_MOUNT_DIR.to_string(),
		read_only: Some(true),
		..Default::default()
	});

	for plugin in plugins {
		let vol_name = plugin_volume_name(plugin);
		volumes.push(Volume {
			name: vol_name.clone(),
			empty_dir: Some(EmptyDirVolumeSource::default()),
			..Default::default()
		});
		mounts.push(VolumeMount {
			name: vol_name,
			mount_path: plugin.wasm_dir.clone(),
			..Default::default()
		});
	}

	(volumes, mounts)
}

#[cfg(test)]
mod tests {
	use super::*;
	use kube::core::ObjectMeta as KubeObjectMeta;
	use reinhardt_cloud_types::crd::{
		PluginCapability, PluginSpec, PluginType, ReinhardtApp, ReinhardtAppSpec,
	};
	use rstest::rstest;

	fn app_with_plugins(plugins: Option<Vec<PluginSpec>>) -> ReinhardtApp {
		ReinhardtApp {
			metadata: KubeObjectMeta {
				name: Some("demo".to_string()),
				namespace: Some("default".to_string()),
				uid: Some("11111111-1111-1111-1111-111111111111".to_string()),
				..Default::default()
			},
			spec: ReinhardtAppSpec {
				image: "demo:latest".to_string(),
				plugins,
				..Default::default()
			},
			status: None,
		}
	}

	#[rstest]
	fn test_build_plugin_configmap_none_when_plugins_absent() {
		// Arrange
		let app = app_with_plugins(None);

		// Act
		let cm = build_plugin_configmap(&app).expect("build should succeed");

		// Assert
		assert!(cm.is_none());
	}

	#[rstest]
	fn test_build_plugin_volumes_empty_when_plugins_absent() {
		// Arrange
		let app = app_with_plugins(None);

		// Act
		let (volumes, mounts) = build_plugin_volumes(&app);

		// Assert
		assert!(volumes.is_empty());
		assert!(mounts.is_empty());
	}

	#[rstest]
	fn test_build_plugin_configmap_contains_single_entry() {
		// Arrange
		let app = app_with_plugins(Some(vec![PluginSpec {
			name: "auth-gate".to_string(),
			wasm_dir: "/var/lib/dentdelion/auth-gate".to_string(),
			plugin_type: PluginType::HttpMiddleware,
			memory_limit_mb: Some(64),
			timeout_ms: Some(500),
			capabilities: vec![PluginCapability::NetworkAccess],
		}]));

		// Act
		let cm = build_plugin_configmap(&app)
			.expect("build should succeed")
			.expect("configmap should be present");

		// Assert
		assert_eq!(cm.metadata.name.as_deref(), Some("demo-dentdelion-plugins"));
		let rendered = cm
			.data
			.as_ref()
			.and_then(|d| d.get(DENTDELION_CONFIG_FILE))
			.expect("rendered document should be present");
		assert!(rendered.contains("[[plugins]]"));
		assert!(rendered.contains("name = \"auth-gate\""));
		assert!(rendered.contains("type = \"http-middleware\""));
		assert!(rendered.contains("memory_limit_mb = 64"));
		assert!(rendered.contains("timeout_ms = 500"));
		assert!(rendered.contains("\"network\""));
	}

	#[rstest]
	fn test_build_plugin_volumes_count_matches_plugins_plus_config() {
		// Arrange
		let plugins = vec![
			PluginSpec {
				name: "a".to_string(),
				wasm_dir: "/p/a".to_string(),
				plugin_type: PluginType::HttpMiddleware,
				memory_limit_mb: None,
				timeout_ms: None,
				capabilities: Vec::new(),
			},
			PluginSpec {
				name: "b".to_string(),
				wasm_dir: "/p/b".to_string(),
				plugin_type: PluginType::GrpcInterceptor,
				memory_limit_mb: None,
				timeout_ms: None,
				capabilities: Vec::new(),
			},
		];
		let app = app_with_plugins(Some(plugins));

		// Act
		let (volumes, mounts) = build_plugin_volumes(&app);

		// Assert
		// 1 config ConfigMap volume + 2 plugin empty-dir volumes
		assert_eq!(volumes.len(), 3);
		assert_eq!(mounts.len(), 3);
		assert_eq!(volumes[0].name, "dentdelion-config");
		assert_eq!(mounts[0].mount_path, DENTDELION_CONFIG_MOUNT_DIR);
		assert_eq!(mounts[1].mount_path, "/p/a");
		assert_eq!(mounts[2].mount_path, "/p/b");
	}

	#[rstest]
	fn test_plugin_capabilities_roundtrip_via_rendered_config() {
		// Arrange
		let plugin = PluginSpec {
			name: "full".to_string(),
			wasm_dir: "/p/full".to_string(),
			plugin_type: PluginType::EventHandler,
			memory_limit_mb: None,
			timeout_ms: None,
			capabilities: vec![
				PluginCapability::NetworkAccess,
				PluginCapability::FilesystemRead,
				PluginCapability::FilesystemWrite,
				PluginCapability::EnvAccess,
			],
		};

		// Act
		let rendered =
			render_plugin_config(std::slice::from_ref(&plugin)).expect("render should succeed");

		// Assert
		for expected in ["network", "fs-read", "fs-write", "env"] {
			assert!(
				rendered.contains(&format!("\"{expected}\"")),
				"expected capability `{expected}` in rendered document: {rendered}",
			);
		}
		assert!(rendered.contains("type = \"event-handler\""));
	}

	#[rstest]
	fn test_build_plugin_configmap_empty_list_returns_none() {
		// Arrange
		let app = app_with_plugins(Some(Vec::new()));

		// Act
		let cm = build_plugin_configmap(&app).expect("build should succeed");

		// Assert
		assert!(cm.is_none());
	}
}
