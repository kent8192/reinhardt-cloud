use std::process::Stdio;
use std::time::Duration;

use crate::fixture::CloudFixture;
use cloud_dashboard::apps::identity::models::Session;
use cloud_dashboard::apps::identity::models::UserAccount;
use cloud_dashboard::apps::identity::persistence::token_hash;
use cloud_dashboard::apps::organization::models::Membership;
use cloud_dashboard::apps::organization::services::OrganizationSummary;
use cloud_dashboard::apps::project::{
	models::{Environment, Project},
	services::{
		DesiredRuntime, EnvironmentKind, EnvironmentSummary, ProjectDetail, ProjectSummary,
	},
};
use reinhardt::db::orm::{Model, connection::DatabaseConnectionLease};
use reqwest::{Client, StatusCode, header};
use rstest::rstest;

fn cookie(response: &reqwest::Response, name: &str) -> String {
	response
		.headers()
		.get_all(header::SET_COOKIE)
		.iter()
		.filter_map(|value| value.to_str().ok())
		.find_map(|value| {
			value
				.split(';')
				.next()
				.filter(|pair| pair.starts_with(&format!("{name}=")))
				.map(str::to_owned)
		})
		.unwrap()
}

#[rstest]
#[tokio::test]
async fn native_identity_reloads_membership_and_revokes_sessions() {
	// Arrange
	let fixture = CloudFixture::new().await;
	fixture.run(&["migrate"]).await;
	for email in ["owner-one@example.test", "owner-two@example.test"] {
		let output = fixture
			.command(&["createplatformadmin", email])
			.env("CLOUD_BOOTSTRAP_PASSWORD", "a-long-test-password")
			.output()
			.await
			.unwrap();
		assert!(
			output.status.success(),
			"{}",
			String::from_utf8_lossy(&output.stderr)
		);
	}
	let address = fixture.origin.strip_prefix("http://").unwrap();
	let mut server = fixture
		.command(&["runserver", address, "--noreload", "--no-wasm"])
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.spawn()
		.unwrap();
	let client = Client::builder()
		.redirect(reqwest::redirect::Policy::none())
		.timeout(Duration::from_secs(5))
		.build()
		.unwrap();
	let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
	loop {
		assert!(
			server.try_wait().unwrap().is_none(),
			"Control Plane exited before readiness"
		);
		if client
			.get(format!("{}/healthz/", fixture.origin))
			.send()
			.await
			.is_ok_and(|response| response.status() == StatusCode::OK)
		{
			break;
		}
		assert!(
			tokio::time::Instant::now() < deadline,
			"Control Plane readiness deadline exceeded"
		);
		tokio::time::sleep(Duration::from_millis(50)).await;
	}
	let unauthenticated = client
		.get(format!("{}/api/v1/organizations/", fixture.origin))
		.send()
		.await
		.unwrap();
	let csrf_cookie = cookie(&unauthenticated, "cloud_csrf");
	let csrf = csrf_cookie.split_once('=').unwrap().1;
	let login = |email: &'static str, origin: &str| {
		client
			.post(format!("{}/login/", fixture.origin))
			.header(header::ORIGIN, origin)
			.header(header::COOKIE, &csrf_cookie)
			.form(&[
				("email", email),
				("password", "a-long-test-password"),
				("csrf_token", csrf),
			])
	};
	// Act
	let missing_csrf = client
		.post(format!("{}/login/", fixture.origin))
		.header(header::ORIGIN, &fixture.origin)
		.form(&[
			("email", "owner-one@example.test"),
			("password", "a-long-test-password"),
		])
		.send()
		.await
		.unwrap();
	let wrong_origin = login("owner-one@example.test", "https://unrelated.example.test")
		.send()
		.await
		.unwrap();
	let first = login("owner-one@example.test", &fixture.origin)
		.send()
		.await
		.unwrap();
	let second = login("owner-two@example.test", &fixture.origin)
		.send()
		.await
		.unwrap();
	let first_session = cookie(&first, "cloud_session");
	let second_session = cookie(&second, "cloud_session");
	let organizations = |session: &str| {
		client
			.get(format!("{}/api/v1/organizations/", fixture.origin))
			.header(header::COOKIE, session)
	};
	let first_orgs = organizations(&first_session)
		.send()
		.await
		.unwrap()
		.json::<Vec<OrganizationSummary>>()
		.await
		.unwrap();
	let second_orgs = organizations(&second_session)
		.send()
		.await
		.unwrap()
		.json::<Vec<OrganizationSummary>>()
		.await
		.unwrap();
	let backend = reinhardt::db::backends::DatabaseConnection::connect_postgres_with_pool_size(
		&fixture.database_url(),
		Some(2),
	)
	.await
	.unwrap();
	let lease = DatabaseConnectionLease::register(backend).unwrap();
	let mut connection = lease.handle();
	let project = Project::new()
		.organization(first_orgs[0].id)
		.display_name("Organization One App")
		.repository_url("https://github.com/example/first")
		.finish();
	let project = Project::objects()
		.create_with_conn(&mut connection, &project)
		.await
		.unwrap();
	let environment = Environment::new()
		.organization(first_orgs[0].id)
		.project(project.id)
		.kind("staging")
		.version(0)
		.desired_runtime("{}")
		.finish();
	let environment = Environment::objects()
		.create_with_conn(&mut connection, &environment)
		.await
		.unwrap();
	let projects_url = format!(
		"{}/api/v1/projects/?organization={}",
		fixture.origin, first_orgs[0].id
	);
	let own_projects = client
		.get(&projects_url)
		.header(header::COOKIE, &first_session)
		.send()
		.await
		.unwrap()
		.json::<Vec<ProjectSummary>>()
		.await
		.unwrap();
	let other_projects = client
		.get(&projects_url)
		.header(header::COOKIE, &second_session)
		.send()
		.await
		.unwrap();
	let detail_url = format!(
		"{}/api/v1/project/?organization={}&project={}",
		fixture.origin, first_orgs[0].id, project.id
	);
	let own_detail = client
		.get(&detail_url)
		.header(header::COOKIE, &first_session)
		.send()
		.await
		.unwrap()
		.json::<ProjectDetail>()
		.await
		.unwrap();
	let other_detail = client
		.get(&detail_url)
		.header(header::COOKIE, &second_session)
		.send()
		.await
		.unwrap();
	let mismatched_detail = client
		.get(format!(
			"{}/api/v1/project/?organization={}&project={}",
			fixture.origin, second_orgs[0].id, project.id
		))
		.header(header::COOKIE, &second_session)
		.send()
		.await
		.unwrap();
	let raw_token = first_session.split_once('=').unwrap().1;
	let stored_sessions = Session::objects()
		.filter(Session::field_token_hash().eq(token_hash(raw_token)))
		.all_with_db(&mut connection)
		.await
		.unwrap();
	Membership::objects()
		.filter(Membership::field_user_id().eq(stored_sessions[0].user_id()))
		.filter(Membership::field_organization_id().eq(first_orgs[0].id))
		.update_fields_with_conn(
			&mut connection,
			[Membership::field_role().assign("invalid")],
		)
		.await
		.unwrap();
	let changed_membership = client
		.get(&projects_url)
		.header(header::COOKIE, &first_session)
		.send()
		.await
		.unwrap();
	Membership::objects()
		.filter(Membership::field_user_id().eq(stored_sessions[0].user_id()))
		.filter(Membership::field_organization_id().eq(first_orgs[0].id))
		.update_fields_with_conn(&mut connection, [Membership::field_role().assign("owner")])
		.await
		.unwrap();
	UserAccount::objects()
		.filter(UserAccount::field_id().eq(stored_sessions[0].user_id()))
		.update_fields_with_conn(&mut connection, [UserAccount::field_active().assign(false)])
		.await
		.unwrap();
	let inactive = organizations(&first_session).send().await.unwrap();
	UserAccount::objects()
		.filter(UserAccount::field_id().eq(stored_sessions[0].user_id()))
		.update_fields_with_conn(&mut connection, [UserAccount::field_active().assign(true)])
		.await
		.unwrap();
	Session::objects()
		.filter(Session::field_id().eq(stored_sessions[0].id))
		.update_fields_with_conn(
			&mut connection,
			[
				Session::field_expires_at()
					.assign(chrono::Utc::now() - chrono::Duration::seconds(1)),
			],
		)
		.await
		.unwrap();
	let expired = organizations(&first_session).send().await.unwrap();
	Session::objects()
		.filter(Session::field_id().eq(stored_sessions[0].id))
		.update_fields_with_conn(
			&mut connection,
			[
				Session::field_expires_at()
					.assign(chrono::Utc::now() + chrono::Duration::hours(24)),
			],
		)
		.await
		.unwrap();
	let logout = client
		.post(format!("{}/logout/", fixture.origin))
		.header(header::ORIGIN, &fixture.origin)
		.header(header::COOKIE, format!("{first_session}; {csrf_cookie}"))
		.form(&[("csrf_token", csrf)])
		.send()
		.await
		.unwrap();
	let replay = organizations(&first_session).send().await.unwrap();
	// Assert
	assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
	assert_eq!(missing_csrf.status(), StatusCode::FORBIDDEN);
	assert_eq!(wrong_origin.status(), StatusCode::FORBIDDEN);
	assert_eq!(first.status(), StatusCode::SEE_OTHER);
	assert_eq!(second.status(), StatusCode::SEE_OTHER);
	assert_eq!(first_orgs.len(), 1);
	assert_eq!(second_orgs.len(), 1);
	assert_ne!(first_orgs[0].id, second_orgs[0].id);
	assert_eq!(
		own_projects,
		vec![ProjectSummary {
			id: project.id,
			organization_id: first_orgs[0].id,
			name: project.display_name.clone(),
			repository: project.repository_url.clone(),
			environments: 1
		}]
	);
	assert_eq!(
		own_detail,
		ProjectDetail {
			id: project.id,
			organization_id: first_orgs[0].id,
			name: project.display_name,
			repository: project.repository_url,
			environments: vec![EnvironmentSummary {
				id: environment.id,
				kind: EnvironmentKind::Staging,
				version: 0,
				desired_runtime: DesiredRuntime::default(),
				latest_operation: None
			}],
		}
	);
	assert_eq!(other_detail.status(), StatusCode::FORBIDDEN);
	assert_eq!(mismatched_detail.status(), StatusCode::FORBIDDEN);
	assert_eq!(other_projects.status(), StatusCode::FORBIDDEN);
	assert_eq!(changed_membership.status(), StatusCode::FORBIDDEN);
	assert_eq!(inactive.status(), StatusCode::UNAUTHORIZED);
	assert_eq!(expired.status(), StatusCode::UNAUTHORIZED);
	assert_eq!(stored_sessions.len(), 1);
	assert_eq!(stored_sessions[0].token_hash, token_hash(raw_token));
	assert_ne!(stored_sessions[0].token_hash, raw_token);
	assert_eq!(logout.status(), StatusCode::SEE_OTHER);
	assert_eq!(replay.status(), StatusCode::UNAUTHORIZED);
}
