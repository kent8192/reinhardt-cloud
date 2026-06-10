//! Auth client route names and path builders.

pub const LOGIN_PAGE_ROUTE: &str = "auth:login_page";
pub const REGISTER_PAGE_ROUTE: &str = "auth:register_page";

pub const LOGIN_PAGE_LOCAL_ROUTE: &str = "login_page";
pub const REGISTER_PAGE_LOCAL_ROUTE: &str = "register_page";

pub const LOGIN_PAGE_PATH: &str = "/login";
pub const REGISTER_PAGE_PATH: &str = "/register";

pub fn path_for(route_name: &str) -> &'static str {
	match route_name {
		LOGIN_PAGE_ROUTE => LOGIN_PAGE_PATH,
		REGISTER_PAGE_ROUTE => REGISTER_PAGE_PATH,
		_ => "/",
	}
}

pub fn oauth_start_path(provider_id: &str) -> Option<String> {
	match provider_id {
		"github" => Some("/api/auth/oauth/github/start/".to_string()),
		_ => None,
	}
}
