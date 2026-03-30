//! SecurityContext builders for hardened pod and container configurations.

use k8s_openapi::api::core::v1::{
	Capabilities, PodSecurityContext, SeccompProfile, SecurityContext,
};

/// Builds a hardened `PodSecurityContext` for isolated workloads.
///
/// Enforces: non-root user (UID/GID 1000), RuntimeDefault seccomp profile.
pub(crate) fn build_pod_security_context() -> PodSecurityContext {
	PodSecurityContext {
		run_as_non_root: Some(true),
		run_as_user: Some(1000),
		run_as_group: Some(1000),
		fs_group: Some(1000),
		seccomp_profile: Some(SeccompProfile {
			type_: "RuntimeDefault".to_string(),
			..Default::default()
		}),
		..Default::default()
	}
}

/// Builds a hardened `SecurityContext` for individual containers.
///
/// Enforces: no privilege escalation, read-only root filesystem,
/// all capabilities dropped.
pub(crate) fn build_container_security_context() -> SecurityContext {
	SecurityContext {
		allow_privilege_escalation: Some(false),
		read_only_root_filesystem: Some(true),
		capabilities: Some(Capabilities {
			drop: Some(vec!["ALL".to_string()]),
			..Default::default()
		}),
		..Default::default()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn pod_security_context_runs_as_non_root() {
		// Arrange & Act
		let ctx = build_pod_security_context();

		// Assert
		assert_eq!(ctx.run_as_non_root, Some(true));
		assert_eq!(ctx.run_as_user, Some(1000));
		assert_eq!(ctx.run_as_group, Some(1000));
		assert_eq!(ctx.fs_group, Some(1000));
	}

	#[rstest]
	fn pod_security_context_has_seccomp_runtime_default() {
		// Arrange & Act
		let ctx = build_pod_security_context();

		// Assert
		let seccomp = ctx.seccomp_profile.unwrap();
		assert_eq!(seccomp.type_, "RuntimeDefault");
	}

	#[rstest]
	fn container_security_context_drops_all_capabilities() {
		// Arrange & Act
		let ctx = build_container_security_context();

		// Assert
		assert_eq!(ctx.allow_privilege_escalation, Some(false));
		assert_eq!(ctx.read_only_root_filesystem, Some(true));
		let caps = ctx.capabilities.unwrap();
		assert_eq!(caps.drop, Some(vec!["ALL".to_string()]));
	}
}
