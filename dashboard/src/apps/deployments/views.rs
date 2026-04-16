//! View functions for deployment endpoints.

use reinhardt::define_views;

define_views! {
	pub mod create_deployment;
	pub mod delete_deployment;
	pub mod deployment_logs;
	pub mod deployment_status;
	pub mod list_deployments;
	pub mod retrieve_deployment;
	pub mod update_deployment;
}
