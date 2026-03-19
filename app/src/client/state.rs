//! Global application state for the WASM client.

use std::cell::RefCell;

use crate::shared::UserInfo;

/// Global application state.
pub struct AppState {
	/// Currently authenticated user, if any.
	pub user: Option<UserInfo>,
	/// Whether the initial auth check is still in progress.
	pub is_loading: bool,
	/// Auth token stored after login.
	pub token: Option<String>,
}

impl Default for AppState {
	fn default() -> Self {
		Self {
			user: None,
			is_loading: true,
			token: None,
		}
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
