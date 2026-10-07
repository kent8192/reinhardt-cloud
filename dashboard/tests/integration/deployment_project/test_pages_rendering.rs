use cloud_dashboard::apps::deployment::services::{
	OperationProgress, OperationSnapshot, OperationState,
};
use cloud_dashboard::apps::project::client::components::projects::{
	ProjectListState, project_list,
};
use cloud_dashboard::apps::project::server::render_document;
use cloud_dashboard::apps::project::services::ProjectSummary;
use cloud_dashboard::apps::project::services::{
	DesiredRuntime, EnvironmentKind, EnvironmentSummary, ProjectDetail,
};
use cloud_dashboard::config::apps::InstalledApp;
use reinhardt::pages::reactive::ReactiveScope;
use reinhardt::pages::ssr::SsrRenderer;
use rstest::rstest;
use scraper::{Html, Selector};
use uuid::Uuid;

#[rstest]
#[case(ProjectListState::Forbidden, "Access denied")]
#[case(ProjectListState::Unavailable, "Unable to load projects")]
#[case(ProjectListState::Ready(Vec::new()), "No projects yet")]
#[tokio::test]
async fn document_serving_preserves_the_explicit_page_state(
	#[case] state: ProjectListState,
	#[case] expected: &str,
) {
	// Arrange
	let template = "<html><body><div id=\"root\"><!--cloud-dashboard-body--></div><!--cloud-dashboard-state--></body></html>";
	// Act
	let output = render_document(template, state, "en").await.unwrap();
	let html = Html::parse_document(&output);
	let state_script = html
		.select(&Selector::parse("#ssr-state").unwrap())
		.next()
		.unwrap()
		.inner_html();
	let snapshot: serde_json::Value = serde_json::from_str(&state_script).unwrap();
	// Assert
	assert_eq!(
		html.select(&Selector::parse("#root h2").unwrap())
			.next()
			.unwrap()
			.text()
			.collect::<String>(),
		expected
	);
	assert!(snapshot.is_object());
}

#[rstest]
#[tokio::test]
async fn project_fields_are_escaped_in_the_published_document() {
	// Arrange
	let template = "<div id=\"root\"><!--cloud-dashboard-body--></div><!--cloud-dashboard-state-->";
	let project = ProjectSummary {
		id: Uuid::from_u128(1),
		organization_id: Uuid::from_u128(2),
		name: "<script>alert('project')</script>".into(),
		repository: "<img src=x onerror=alert('repository')>".into(),
		environments: 2,
	};
	// Act
	let output = render_document(
		template,
		ProjectListState::Ready(vec![project.clone()]),
		"ja",
	)
	.await
	.unwrap();
	let html = Html::parse_fragment(&output);
	let cells = html
		.select(&Selector::parse("tbody td").unwrap())
		.map(|cell| cell.text().collect::<String>())
		.collect::<Vec<_>>();
	// Assert
	assert_eq!(cells, [project.name, project.repository, "2".into()]);
	assert_eq!(
		html.select(&Selector::parse("script:not(#ssr-state), img").unwrap())
			.count(),
		0
	);
}

#[rstest]
#[case(
	"en",
	"Production",
	"Restart: Outcome uncertain",
	"Further changes wait for cluster reconciliation."
)]
#[case(
	"ja",
	"本番",
	"再起動: 結果が不確定",
	"クラスタの状態確認が完了するまで、次の変更を待機します。"
)]
#[tokio::test]
async fn project_detail_renders_uncertain_operation_without_claiming_readiness(
	#[case] locale: &str,
	#[case] kind: &str,
	#[case] state: &str,
	#[case] gate: &str,
) {
	// Arrange
	let environment = Uuid::from_u128(3);
	let detail = ProjectDetail {
		id: Uuid::from_u128(1),
		organization_id: Uuid::from_u128(2),
		name: "<script>Project</script>".into(),
		repository: "https://github.com/example/app".into(),
		environments: vec![EnvironmentSummary {
			id: environment,
			kind: EnvironmentKind::Production,
			version: 7,
			desired_runtime: DesiredRuntime {
				replicas: 2,
				..DesiredRuntime::default()
			},
			latest_operation: Some(OperationSnapshot {
				kind: "restart".into(),
				created_at: chrono::Utc::now(),
				progress: OperationProgress {
					id: Uuid::from_u128(4),
					environment_id: environment,
					environment_version: 7,
					state: OperationState::Uncertain,
				},
			}),
		}],
	};
	let template = "<div id=\"root\"><!--cloud-dashboard-body--></div><!--cloud-dashboard-state-->";
	// Act
	let output = render_document(template, ProjectListState::Detail(Box::new(detail)), locale)
		.await
		.unwrap();
	let html = Html::parse_fragment(&output);
	let text = |selector: &str| {
		html.select(&Selector::parse(selector).unwrap())
			.next()
			.unwrap()
			.text()
			.collect::<String>()
	};
	// Assert
	assert_eq!(text("h1"), "<script>Project</script>");
	assert_eq!(text("article h3"), kind);
	assert_eq!(text("article h4 + p"), state);
	assert_eq!(text("[role=status]"), gate);
	assert_eq!(
		html.select(&Selector::parse("script:not(#ssr-state)").unwrap())
			.count(),
		0
	);
	assert_eq!(
		html.select(&Selector::parse("article dd").unwrap())
			.map(|node| node.text().collect::<String>())
			.collect::<Vec<_>>(),
		["7", "2"]
	);
}

#[rstest]
#[case("en", "Projects", "Sign in to your organization")]
#[case("ja", "プロジェクト", "組織にサインイン")]
#[tokio::test]
async fn ssr_renders_catalog_backed_access_state(
	#[case] locale: &str,
	#[case] title: &str,
	#[case] heading: &str,
) {
	// Arrange
	let scope = ReactiveScope::new();
	let page = scope.enter(|| project_list(ProjectListState::AuthenticationRequired, locale));
	let mut renderer = SsrRenderer::new();
	// Act
	let html = renderer.render_view(&page).await;
	let document = Html::parse_fragment(&html);
	let h1 = Selector::parse("h1").unwrap();
	let h2 = Selector::parse("h2").unwrap();
	// Assert
	assert_eq!(
		document
			.select(&h1)
			.next()
			.unwrap()
			.text()
			.collect::<String>(),
		title
	);
	assert_eq!(
		document
			.select(&h2)
			.next()
			.unwrap()
			.text()
			.collect::<String>(),
		heading
	);
	assert_eq!(
		document
			.select(&Selector::parse("header button").unwrap())
			.count(),
		2
	);
}

#[rstest]
fn all_eight_app_owners_are_registered() {
	// Arrange
	let expected = [
		"identity",
		"organization",
		"project",
		"source",
		"deployment",
		"cluster",
		"secret",
		"observability",
	];
	// Act
	let labels = InstalledApp::all_labels();
	// Assert
	assert_eq!(labels, expected);
}
