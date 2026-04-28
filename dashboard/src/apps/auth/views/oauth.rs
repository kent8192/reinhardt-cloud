//! OAuth/social login view aggregator.

use reinhardt::flatten_imports;

flatten_imports! {
	pub mod callback;
	pub mod providers;
	pub mod start;
	pub mod unlink;
}
