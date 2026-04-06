//! Tests for clusters app.

pub mod e2e {
	pub mod test_cluster_crud;
	pub mod test_cluster_ownership;
	pub mod test_cluster_pagination;
}
pub mod unit {
	pub mod test_cluster_model;
	pub mod test_cluster_property;
	pub mod test_request_validation;
	pub mod test_serializer;
}
