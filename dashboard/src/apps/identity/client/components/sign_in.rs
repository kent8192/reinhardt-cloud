//! Password sign-in works before and after Pages hydration.
use crate::client::components::shell::{STYLES, shell};
use crate::client::screens::{context, csrf_token, workspace};
use reinhardt::pages::i18n::I18nContext;
use reinhardt::pages::{Page, component, page};
#[component("/login/", name = "sign-in-page")]
pub fn sign_in() -> Page {
	let context = context();
	let failed = false;
	shell(
		workspace(
			None,
			sign_in_form(failed, csrf_token(), context.clone()),
			false,
			context.clone(),
			csrf_token(),
		),
		context,
	)
}
pub fn sign_in_form(failed: bool, csrf: String, context: I18nContext) -> Page {
	page!({
		p {
			class: STYLES.status(),
			{ context.translate("Authentication required") }
		}
		h2 {
			class: STYLES.panel_title(),
			{ context.translate("Sign in to your organization") }
		}
		p {
			class: STYLES.copy(),
			{ context.translate(
				"Your projects and environments are visible after your organization membership is verified.",
			) }
		}
		if failed {
			p {
				role: "alert",
				class: STYLES.copy(),
				{ context.translate("The email or password is incorrect.") }
			}
		}
		form {
			action: "/login/",
			method: "post",
			class: STYLES.form(),
			input {
				type: "hidden",
				name: "csrf_token",
				value: csrf
			}
			label {
				for: "email",
				class: STYLES.label(),
				{ context.translate("Email") }
			}
			input {
				id: "email",
				name: "email",
				type: "email",
				autocomplete: "username",
				required: true,
				class: STYLES.input()
			}
			label {
				for: "password",
				class: STYLES.label(),
				{ context.translate("Password") }
			}
			input {
				id: "password",
				name: "password",
				type: "password",
				autocomplete: "current-password",
				required: true,
				class: STYLES.input()
			}
			button {
				type: "submit",
				class: STYLES.primary(),
				{ context.translate("Sign in") }
			}
		}
	})
}
