//! gRPC services for the logs application.

use reinhardt::grpc::GrpcRouter;

/// Return the gRPC services contributed by this application.
pub fn grpc_services() -> GrpcRouter {
	GrpcRouter::new()
}
