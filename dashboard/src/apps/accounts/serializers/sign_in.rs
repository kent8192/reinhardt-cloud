//! Sign-in notices shown on the sign-in page.

use serde::{Deserialize, Serialize};

/// What the sign-in page tells a visitor whose last attempt did not succeed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignInNotice {
	/// The Control Plane does not admit this GitHub account.
	NotInvited {
		/// GitHub login of the visitor.
		login: String,
	},
	/// The sign-in did not complete. The page does not say why.
	Failed,
}
