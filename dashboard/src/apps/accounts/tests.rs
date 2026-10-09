//! Tests of the accounts application.

mod support;

mod unit {
	mod test_admin;
	mod test_sign_up_policy;
	mod test_token_crypto;
}

mod integration {
	mod test_provider_tokens;
	mod test_sign_up;
	mod test_users;
}
