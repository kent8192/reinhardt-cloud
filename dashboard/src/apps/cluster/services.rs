//! Cluster capability and organization allocation admission.

#[derive(Debug, Clone, Copy)]
pub struct ClusterCapabilities {
	pub rootless_builds: bool,
	pub ingress: bool,
	pub dns: bool,
	pub namespace_issuer: bool,
	pub cpu_autoscaling: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClusterError {
	MissingHttps,
	MissingBuildIsolation,
	UnsupportedAutoscaling,
	ReplicaLimit,
}

impl ClusterCapabilities {
	pub fn validate_registration(&self) -> Result<(), ClusterError> {
		if !(self.ingress && self.dns && self.namespace_issuer) {
			return Err(ClusterError::MissingHttps);
		}
		if !self.rootless_builds {
			return Err(ClusterError::MissingBuildIsolation);
		}
		Ok(())
	}

	pub fn admit_scale(
		&self,
		replicas: u32,
		limit: u32,
		autoscaling: bool,
	) -> Result<(), ClusterError> {
		if replicas == 0 || replicas > limit {
			return Err(ClusterError::ReplicaLimit);
		}
		if autoscaling && !self.cpu_autoscaling {
			return Err(ClusterError::UnsupportedAutoscaling);
		}
		Ok(())
	}
}
