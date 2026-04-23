//! View functions for cluster endpoints.

use reinhardt::flatten_imports;

flatten_imports! {
	pub mod create_cluster;
	pub mod delete_cluster;
	pub mod list_clusters;
	pub mod retrieve_cluster;
	pub mod rotate_token;
	pub mod update_cluster;
}
