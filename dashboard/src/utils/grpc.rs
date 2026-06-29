//! Shared gRPC helpers for dashboard server-side integrations.

#[derive(Clone)]
pub(crate) struct DashboardGrpcAuthInterceptor {
	value: tonic::metadata::MetadataValue<tonic::metadata::Ascii>,
}

impl tonic::service::Interceptor for DashboardGrpcAuthInterceptor {
	fn call(
		&mut self,
		mut request: tonic::Request<()>,
	) -> Result<tonic::Request<()>, tonic::Status> {
		request
			.metadata_mut()
			.insert("authorization", self.value.clone());
		Ok(request)
	}
}

pub(crate) fn dashboard_grpc_auth_interceptor(
	token: &str,
) -> Result<DashboardGrpcAuthInterceptor, tonic::metadata::errors::InvalidMetadataValue> {
	let mut value: tonic::metadata::MetadataValue<tonic::metadata::Ascii> =
		format!("Bearer {token}").parse()?;
	value.set_sensitive(true);
	Ok(DashboardGrpcAuthInterceptor { value })
}

#[cfg(test)]
mod tests {
	use super::dashboard_grpc_auth_interceptor;

	#[test]
	fn dashboard_grpc_auth_interceptor_marks_authorization_sensitive() {
		let interceptor = dashboard_grpc_auth_interceptor("secret-token")
			.expect("authorization metadata should parse");

		assert!(interceptor.value.is_sensitive());
		assert_eq!(
			interceptor
				.value
				.to_str()
				.expect("metadata should be ascii"),
			"Bearer secret-token"
		);
	}
}
