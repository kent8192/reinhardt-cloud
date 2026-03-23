//! Global application state for the WASM client.
//!
//! Authentication state (user info, token) is managed by
//! `reinhardt::pages::auth::AuthState` and `localStorage`.
//! This module only tracks non-auth application state.

use std::cell::RefCell;

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

/// Reset global state to defaults.
pub fn init_app_state() {
	APP_STATE.with(|s| {
		*s.borrow_mut() = AppState::default();
	});
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
