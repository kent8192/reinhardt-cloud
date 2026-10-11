//! The outcome of confirming a Login Link.

use serde::{Deserialize, Serialize};

/// How confirming a Login Link ended.
///
/// A link that is unknown, used, expired, or belongs to a deactivated User all
/// answer `Rejected` with the same response (SR-16); the page cannot tell them
/// apart and neither can an attacker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoginLinkOutcome {
	/// The browser is now signed in; the session cookie was set.
	SignedIn,
	/// The link did not sign anyone in.
	Rejected,
}
