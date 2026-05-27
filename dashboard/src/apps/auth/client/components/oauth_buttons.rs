//! OAuth provider sign-in buttons.
//!
//! Renders one anchor per supported provider (today: GitHub only — see
//! #428 / #440 for GitLab follow-up). Each button is a plain link to
//! `/api/auth/oauth/{provider}/start/` so the browser performs a normal
//! navigation, the server issues a 302 to the provider's authorize URL,
//! and the user lands back on `/api/auth/oauth/{provider}/callback/`.
//!
//! The button is unconditionally rendered. If the provider is not
//! configured server-side (no `REINHARDT_CLOUD_OAUTH_GITHUB_*` env
//! vars), the `start` endpoint returns 404, surfacing the
//! misconfiguration to the operator rather than silently hiding the
//! button. Once #440 lands and we have multiple providers, this should
//! switch to fetching the discovery endpoint
//! (`/api/auth/oauth/providers/`) to render only the enabled set.

use reinhardt::pages::component::Page;
use reinhardt::pages::page;

/// Render the OAuth provider button group below the password form.
///
/// `verb` is the leading word ("Sign in" on login, "Sign up" on register)
/// so the button reads naturally in context.
pub fn oauth_buttons(verb: &'static str) -> Page {
	page!(|verb: &'static str| {
		div {
			class: "mt-6",
			div {
				class: "relative",
				div {
					class: "absolute inset-0 flex items-center",
					span {
						class: "w-full border-t border-gray-200",
					}
				}
				div {
					class: "relative flex justify-center text-xs uppercase",
					span {
						class: "bg-white px-2 text-gray-500",
						"Or continue with"
					}
				}
			}
			div {
				class: "mt-4 grid grid-cols-1 gap-2",
				a {
					href: "/api/auth/oauth/github/start/",
					class: "inline-flex items-center justify-center w-full py-2.5 text-sm font-medium border border-gray-300 rounded-md hover:bg-gray-50",
					{
						format!("{verb} with GitHub")
					}
				}
			}
		}
	})(verb)
}
