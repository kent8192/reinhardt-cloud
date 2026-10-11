//! The signed-in User as the Dashboard sees them.

use serde::{Deserialize, Serialize};

/// Profile data of the signed-in User.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Viewer {
	/// Current GitHub login.
	pub github_login: String,
	/// Name shown in the Dashboard.
	pub display_name: String,
	/// Whether the User is Staff.
	pub is_staff: bool,
}
