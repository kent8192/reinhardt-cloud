//! Kubernetes resource builders for operator-managed resources.

pub(crate) mod cache;
pub(crate) mod database;
pub(crate) mod deployment;
pub(crate) mod grpc;
pub(crate) mod i18n;
pub(crate) mod ingress;
pub(crate) mod labels;
pub(crate) mod mail;
pub(crate) mod migration;
pub(crate) mod service;
pub(crate) mod storage;
pub(crate) mod worker;

// Re-exports for backward compatibility
pub(crate) use database::{build_db_secret, build_db_service, build_db_statefulset};
pub(crate) use deployment::build_deployment;
pub(crate) use ingress::build_ingress;
pub(crate) use migration::build_migration_job;
pub(crate) use service::build_service;

/// Validates that a port number is within the valid TCP/UDP range (1-65535).
pub(crate) fn validate_port(field: &'static str, port: i32) -> Result<i32, crate::error::Error> {
	if (1..=65535).contains(&port) {
		Ok(port)
	} else {
		Err(crate::error::Error::InvalidPort { field, port })
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_validate_port_accepts_boundary_values() {
		// Arrange / Act / Assert
		assert_eq!(validate_port("port", 1).unwrap(), 1);
		assert_eq!(validate_port("port", 65535).unwrap(), 65535);
	}
}
