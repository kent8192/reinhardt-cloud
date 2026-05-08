//! Global application state for the WASM client.
//!
//! Authentication state (user info, token) is managed by
//! `reinhardt::pages::auth::AuthState` and `sessionStorage`.
//! This module only tracks non-auth application state.

use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::collections::HashMap;

/// Global application state.
pub struct AppState {
	/// Whether the initial auth check is still in progress.
	pub is_loading: bool,
}

impl Default for AppState {
	fn default() -> Self {
		Self { is_loading: true }
	}
}

thread_local! {
	static APP_STATE: RefCell<AppState> = RefCell::new(AppState::default());
}

/// Register the SPA route names that the dashboard's `dashboard_shell`
/// layout needs to resolve via `ResolvedUrls`.
///
/// The native side wires this through `make_router()` via
/// `register_client_reverser(unified.client_ref().to_reverser())`
/// (see `crate::config::urls::make_router`). On the WASM target the
/// `#[routes]` macro at upstream `f0dd166c` emits only the consumer
/// (`__url_resolver_support::ResolvedUrls`), not the registrar — so
/// without this manual call the first WASM render of `dashboard_shell`
/// panics at `ResolvedUrls::from_global()` with "Global client reverser
/// not registered. Ensure the #[routes] function has been called."
/// Refs `kent8192/reinhardt-cloud#574`,
/// `kent8192/reinhardt-web#4221` (7th SPA navigation regression).
///
/// Hooked into `init_app_state()` (driven by
/// `ClientLauncher::before_launch`) so it runs unconditionally before
/// any route handler renders. The patterns mirror
/// [`crate::client::router::init_router`]'s `named_route` calls
/// byte-for-byte — drift would re-introduce the panic.
#[cfg(target_arch = "wasm32")]
fn register_client_url_reverser() {
	use reinhardt::{ClientUrlReverser, register_client_reverser};

	let patterns = HashMap::from([
		("dashboard:home".to_string(), "/".to_string()),
		("auth:login_page".to_string(), "/login".to_string()),
		("auth:register_page".to_string(), "/register".to_string()),
		("dashboard:clusters".to_string(), "/clusters".to_string()),
		(
			"dashboard:deployments".to_string(),
			"/deployments".to_string(),
		),
	]);
	register_client_reverser(ClientUrlReverser::new(patterns));
}

/// Reset global state to defaults.
///
/// Also registers the WASM-side client URL reverser (#574) when running
/// on `wasm32-unknown-unknown`. Driven by `ClientLauncher::before_launch`
/// so it executes before any layout renders.
pub fn init_app_state() {
	APP_STATE.with(|s| {
		*s.borrow_mut() = AppState::default();
	});
	#[cfg(target_arch = "wasm32")]
	register_client_url_reverser();
}

/// Read-only access to the global application state.
pub fn with_app_state<F, R>(f: F) -> R
where
	F: FnOnce(&AppState) -> R,
{
	APP_STATE.with(|s| f(&s.borrow()))
}

/// Mutable access to the global application state.
pub fn with_app_state_mut<F, R>(f: F) -> R
where
	F: FnOnce(&mut AppState) -> R,
{
	APP_STATE.with(|s| f(&mut s.borrow_mut()))
}
