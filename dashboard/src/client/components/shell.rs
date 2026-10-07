//! Shared responsive presentation and catalog-backed localization.

use reinhardt::pages::i18n::{I18nContext, MessageCatalog, TranslationContext};
use reinhardt::pages::{Page, page, style_def};

#[style_def]
pub static STYLES: ShellStyles = style! {
	.document {
		margin: 0;
	}
	.shell {
		margin: 0;
		min-height: 100vh;
		background: #f6f7fb;
		color: #192437;
		font-family: [system-ui, sans-serif];
		font-size: 15px;
		line-height: 1.6;
		.header {
			display: flex;
			align-items: center;
			justify-content: space-between;
			padding: (18px, 32px);
			border-bottom: (1px, solid, #e1e6ef);
			background: #ffffff;
		}
		.brand {
			color: inherit;
			font-weight: 700;
			font-size: 18px;
			letter-spacing: -0.3px;
			text-decoration: none;
		}
		.controls {
			display: flex;
			gap: 8px;
		}
		.language {
			border: (1px, solid, #d2d9e5);
			border-radius: 6px;
			padding: (6px, 12px);
			color: inherit;
			background: transparent;
			cursor: pointer;
			&:focus-visible {
				outline: (3px, solid, #2870e6);
				outline-offset: 3px;
			}
		}
		.main {
			max-width: 1080px;
			margin: (0, auto);
			padding: (56px, 32px);
		}
		.eyebrow {
			color: #64748b;
			font-size: 13px;
			font-weight: 600;
		}
		.title {
			font-size: 32px;
			letter-spacing: -0.8px;
			margin: (8px, 0);
		}
		.description {
			color: #64748b;
			margin: (0, 0, 32px);
		}
		.panel {
			border: (1px, solid, #dfe5ef);
			border-radius: 10px;
			padding: 40px;
			background: #ffffff;
		}
		.status {
			color: #2463c6;
			font-size: 13px;
			font-weight: 600;
		}
		.panel_title {
			margin: (12px, 0);
			font-size: 21px;
		}
		.copy {
			color: #64748b;
			max-width: 560px;
		}
		.form {
			display: grid;
			gap: 12px;
			max-width: 420px;
			margin-top: 24px;
		}
		.label {
			font-weight: 600;
		}
		.input {
			box-sizing: border-box;
			width: 100%;
			padding: (10px, 12px);
			border: (1px, solid, #cbd5e1);
			border-radius: 6px;
			background: transparent;
			color: inherit;
			font: inherit;
			&:focus-visible {
				outline: (3px, solid, #2870e6);
				outline-offset: 2px;
			}
		}
		.primary {
			padding: (10px, 16px);
			border: (1px, solid, #2463c6);
			border-radius: 6px;
			background: #2463c6;
			color: #ffffff;
			font: inherit;
			cursor: pointer;
			&:focus-visible {
				outline: (3px, solid, #2870e6);
				outline-offset: 3px;
			}
		}
		.link {
			color: #2463c6;
			&:focus-visible {
				outline: (3px, solid, #2870e6);
				outline-offset: 3px;
			}
		}
		.table {
			width: 100%;
			th {
				text-align: left;
				padding: 12px;
				border-bottom: (1px, solid, #e1e6ef);
			}
			td {
				padding: (16px, 12px);
				border-bottom: (1px, solid, #e1e6ef);
			}
		}
		.environment_grid {
			display: grid;
			grid-template-columns: (1fr, 1fr);
			gap: 20px;
			margin-top: 24px;
		}
		.environment_card {
			min-width: 0;
			border: (1px, solid, #dfe5ef);
			border-radius: 8px;
			padding: 24px;
		}
		.identity {
			font-size: 12px;
			color: #64748b;
			word-break: break-all;
		}
		.facts {
			display: grid;
			grid-template-columns: (1fr, 1fr);
			gap: 8px;
			dt {
				color: #64748b;
			}
			dd {
				margin: 0;
				font-weight: 600;
			}
		}
		.operation_title {
			margin: (20px, 0, 8px);
			font-size: 13px;
		}
		.footer {
			margin-top: 24px;
			color: #64748b;
			font-size: 13px;
		}
	}
	@media (max-width: 640px) {
		.shell {
			.header {
				padding: 16px;
			}
			.main {
				padding: (32px, 16px);
			}
			.panel {
				padding: 24px;
				overflow-x: auto;
			}
			.title {
				font-size: 28px;
			}
			.environment_grid {
				grid-template-columns: 1fr;
			}
		}
	}@media (prefers-color-scheme: dark) {
		.shell {
			background: #111827;
			color: #e5edf8;
			.header {
				background: #182234;
				border-color: #334155;
			}
			.panel {
				background: #182234;
				border-color: #334155;
			}
			.copy {
				color: #a5b4c9;
			}
			.description {
				color: #a5b4c9;
			}
			.eyebrow {
				color: #a5b4c9;
			}
			.footer {
				color: #a5b4c9;
			}
			.status {
				color: #82b1ff;
			}
			.language {
				border-color: #475569;
			}
			.link {
				color: #82b1ff;
			}
			.environment_card {
				border-color: #334155;
			}
			.identity {
				color: #a5b4c9;
			}
			.facts {
				dt {
					color: #a5b4c9;
				}
			}
		}
	}
};

pub fn translations(locale: &str) -> I18nContext {
	let mut context = TranslationContext::new(locale, "en");
	let mut japanese = MessageCatalog::new("ja");
	for (message, translated) in [
		("Workspace", "ワークスペース"),
		("Projects", "プロジェクト"),
		(
			"Your applications, from source to production.",
			"ソースから本番運用まで、アプリケーションを管理します。",
		),
		("Authentication required", "認証が必要です"),
		("Sign in to your organization", "組織にサインイン"),
		(
			"Your projects and environments are visible after your organization membership is verified.",
			"組織への所属を確認した後、プロジェクトと環境を表示します。",
		),
		("No projects yet", "プロジェクトはまだありません"),
		(
			"Connect a repository to publish your first application.",
			"リポジトリを接続して、最初のアプリケーションを公開しましょう。",
		),
		("Project", "プロジェクト"),
		("Repository", "リポジトリ"),
		("Environments", "環境"),
		("All projects", "プロジェクト一覧"),
		("No environments yet", "環境はまだありません"),
		(
			"Desired inputs. Readiness is reported separately.",
			"設定された構成です。稼働状態は別に確認します。",
		),
		("Production", "本番"),
		("Staging", "ステージング"),
		("Preview", "プレビュー"),
		("Configuration version", "構成バージョン"),
		("Desired replicas", "設定レプリカ数"),
		("CPU autoscaling", "CPU自動スケーリング"),
		("Latest operation", "最新の操作"),
		("No operations yet", "操作はまだありません"),
		("Operation", "操作"),
		("Restart", "再起動"),
		("Scale", "スケーリング"),
		("Queued", "待機中"),
		("Building", "ビルド中"),
		("Migrating", "マイグレーション中"),
		("Applying", "適用中"),
		("Verifying", "検証中"),
		("Outcome uncertain", "結果が不確定"),
		("Succeeded", "成功"),
		("Failed", "失敗"),
		("Cancelled", "キャンセル済み"),
		(
			"Further changes wait for cluster reconciliation.",
			"クラスタの状態確認が完了するまで、次の変更を待機します。",
		),
		("Access denied", "アクセスできません"),
		(
			"Your current membership does not allow access to this organization.",
			"現在の権限では、この組織にアクセスできません。",
		),
		("Unable to load projects", "プロジェクトを読み込めません"),
		("Loading…", "読み込み中…"),
		(
			"Enter a replica count between 1 and 1000.",
			"レプリカ数は1〜1000の整数を入力してください。",
		),
		("Runtime controls", "実行設定"),
		("Submitting…", "送信中…"),
		(
			"Operation queued. Readiness is reported separately.",
			"操作を受け付けました。稼働状態は別途報告されます。",
		),
		("Email", "メールアドレス"),
		("Password", "パスワード"),
		("Sign in", "サインイン"),
		("Sign out", "サインアウト"),
		("Choose an organization", "組織を選択"),
		(
			"The email or password is incorrect.",
			"メールアドレスまたはパスワードが正しくありません。",
		),
		(
			"Try again when the service is available.",
			"サービスが利用可能になってから、もう一度お試しください。",
		),
	] {
		japanese.add_translation(message, translated);
	}
	context
		.add_catalog("ja", japanese)
		.expect("Japanese is a valid locale");
	I18nContext::new(context)
}

pub fn shell(body: Page, context: I18nContext) -> Page {
	let english = context.clone();
	let japanese = context;
	page!({
		div {
			class: STYLES.shell(),
			header {
				class: STYLES.header(),
				a {
					class: STYLES.brand(),
					href: "/",
					"Reinhardt Cloud"
				}
				div {
					class: STYLES.controls(),
					button {
						class: STYLES.language(),
						@click: move |_| {
							select_locale(&english, "en");
						},
						"English"
					}
					button {
						class: STYLES.language(),
						@click: move |_| {
							select_locale(&japanese, "ja");
						},
						"日本語"
					}
				}
			}
			main {
				class: STYLES.main(),
				{ body }
			}
		}
	})
}

fn select_locale(context: &I18nContext, locale: &str) {
	context
		.set_locale(locale)
		.expect("The UI exposes valid locale tags");
	#[cfg(wasm)]
	if let Some(window) = web_sys::window() {
		use wasm_bindgen::JsCast;
		if let Some(document) = window.document() {
			if let Some(html) = document.document_element() {
				html.set_attribute("lang", locale)
					.expect("The lang attribute is valid");
			}
			if let Ok(document) = document.dyn_into::<web_sys::HtmlDocument>() {
				let secure = window.location().protocol().ok().as_deref() == Some("https:");
				let cookie = format!(
					"cloud_locale={locale}; Path=/; Max-Age=31536000; SameSite=Lax{}",
					if secure { "; Secure" } else { "" }
				);
				let _result = document.set_cookie(&cookie);
			}
		}
	}
}
