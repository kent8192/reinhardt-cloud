//! View functions for deployment endpoints.

use reinhardt::flatten_imports;

flatten_imports! {
	pub mod create_deployment;
	pub mod delete_deployment;
	pub mod deployment_logs;
	pub mod deployment_status;
	pub mod list_deployments;
	pub mod retrieve_deployment;
	pub mod update_deployment;
}
