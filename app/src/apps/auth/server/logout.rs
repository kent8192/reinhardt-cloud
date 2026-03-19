//! Logout server function for frontend session termination.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};

/// Invalidate the current session.
///
/// With token-based authentication the server does not maintain session
/// state, so logout is effectively a no-op on the server side. The WASM
/// client is responsible for discarding the stored token. This endpoint
/// exists so the client has a consistent RPC surface for the auth flow.
#[server_fn]
pub async fn logout() -> Result<bool, ServerFnError> {
	// Token-based auth is stateless on the server; the client
	// discards its stored token to complete the logout.
	Ok(true)
}
