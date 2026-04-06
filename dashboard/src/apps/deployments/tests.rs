//! Tests for deployments app.

pub mod e2e {
	pub mod test_deployment_crud;
	pub mod test_deployment_ownership;
	pub mod test_deployment_pagination;
}
pub mod unit {
	pub mod test_deployment_model;
	pub mod test_deployment_property;
	pub mod test_request_validation;
	pub mod test_serializer;
}
