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
	let value = format!("Bearer {token}").parse()?;
	Ok(DashboardGrpcAuthInterceptor { value })
}
