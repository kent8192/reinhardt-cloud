//! View functions for cluster endpoints.

use reinhardt::define_views;

define_views! {
	pub mod create_cluster;
	pub mod delete_cluster;
	pub mod list_clusters;
	pub mod retrieve_cluster;
	pub mod update_cluster;
}
