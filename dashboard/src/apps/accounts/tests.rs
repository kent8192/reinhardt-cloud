//! Tests of the accounts application.

mod server_support;
mod support;

mod unit {
	mod test_access_gate;
	mod test_admin;
	mod test_base_user;
	mod test_commands;
	mod test_cookies;
	mod test_cross_site_guard;
	mod test_request_origins;
	mod test_security_headers;
	mod test_sign_up_policy;
	mod test_token_crypto;
}

mod integration {
	mod test_github_flow;
	mod test_login_link_sign_in;
	mod test_login_links;
	mod test_provider_token_refresh;
	mod test_provider_tokens;
	mod test_repoint;
	mod test_request_surface;
	mod test_sessions;
	mod test_sign_in;
	mod test_sign_up;
	mod test_staff;
	mod test_user_recovery;
	mod test_users;
}
