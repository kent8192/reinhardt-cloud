//! Target-neutral routes owned by this App.

use reinhardt::UnifiedRouter;

pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
}
