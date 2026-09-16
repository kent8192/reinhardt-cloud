//! Submission regression tests shared by the generated dashboard forms.

use reinhardt::Validate;
use reinhardt::pages::prelude::{UseFormAsyncSubmitOutcome, use_form};
use reinhardt::pages::reactive::ReactiveScope;
use reinhardt::pages::server_fn::ServerFnError;
use rstest::rstest;

use crate::apps::auth::serializers::register::{RegisterRequest, RegisterRequestClientForm};
use crate::apps::clusters::server_fn::{
	UpdateClusterFormRequest, UpdateClusterFormRequestClientForm,
};
use crate::apps::deployments::server_fn::{
	CreateDeploymentFormRequest, CreateDeploymentFormRequestClientForm,
	UpdateDeploymentFormRequest, UpdateDeploymentFormRequestClientForm,
	UpdateDeploymentStatusFormRequest, UpdateDeploymentStatusFormRequestClientForm,
};
use crate::apps::github::server_fn::{
	GitHubRepositoryImportRequest, GitHubRepositoryImportRequestClientForm,
};

#[rstest]
#[tokio::test]
async fn registration_normalizes_before_client_validation_and_server_dispatch() {
	// Arrange
	let request = RegisterRequest {
		username: format!(" {} ", "a".repeat(32)),
		email: " Alice@Example.COM ".to_owned(),
		password: "  secret password  ".to_owned(),
	};
	let scope = ReactiveScope::new();
	let runtime = scope.enter(|| {
		let form = RegisterRequestClientForm::new().with_defaults(request.clone());
		use_form(&form).build()
	});
	assert!(
		runtime.trigger().is_err(),
		"raw whitespace exceeds the DTO contract"
	);

	// Act
	RegisterRequestClientForm::normalize_values(&runtime);
	let normalized = RegisterRequestClientForm::to_request(&runtime);
	let dispatched = normalized.clone();
	let outcome = runtime
		.submit_server_fn(move || async move {
			dispatched.validate().map_err(ServerFnError::from)?;
			Ok(dispatched)
		})
		.await
		.expect("normalized request must reach the server");

	// Assert
	assert_eq!(
		outcome,
		UseFormAsyncSubmitOutcome::Submitted(normalized.clone())
	);
	assert_eq!(request.normalized(), normalized);
	assert_eq!(normalized.username, "a".repeat(32));
	assert_eq!(normalized.email, "alice@example.com");
	assert_eq!(normalized.password, "  secret password  ");
}

#[rstest]
#[tokio::test]
async fn cluster_update_normalizes_before_client_validation_and_server_dispatch() {
	// Arrange
	let request = UpdateClusterFormRequest {
		cluster_id: "41".to_owned(),
		name: format!(" {} ", "a".repeat(63)),
		api_url: " https://cluster.example.test:6443 ".to_owned(),
		is_active: true,
	};
	let scope = ReactiveScope::new();
	let runtime = scope.enter(|| {
		let form = UpdateClusterFormRequestClientForm::new().with_defaults(request.clone());
		use_form(&form).build()
	});
	assert!(
		runtime.trigger().is_err(),
		"raw whitespace exceeds the DTO contract"
	);

	// Act
	UpdateClusterFormRequestClientForm::normalize_values(&runtime);
	let normalized = UpdateClusterFormRequestClientForm::to_request(&runtime);
	let dispatched = normalized.clone();
	let outcome = runtime
		.submit_server_fn(move || async move {
			dispatched.validate().map_err(ServerFnError::from)?;
			Ok(dispatched)
		})
		.await
		.expect("normalized request must reach the server");

	// Assert
	assert_eq!(
		outcome,
		UseFormAsyncSubmitOutcome::Submitted(normalized.clone())
	);
	assert_eq!(request.normalized(), normalized);
	assert_eq!(normalized.name, "a".repeat(63));
	assert_eq!(normalized.api_url, "https://cluster.example.test:6443");
}

#[rstest]
#[tokio::test]
async fn deployment_create_normalizes_before_client_validation_and_server_dispatch() {
	// Arrange
	let request = CreateDeploymentFormRequest {
		project_name: format!(" {} ", "a".repeat(63)),
		cluster_id: "41".to_owned(),
		image: format!(" {} ", "i".repeat(512)),
		project_yaml: "  preserved manifest bytes\n".to_owned(),
	};
	let scope = ReactiveScope::new();
	let runtime = scope.enter(|| {
		let form = CreateDeploymentFormRequestClientForm::new().with_defaults(request.clone());
		use_form(&form).build()
	});
	assert!(
		runtime.trigger().is_err(),
		"raw whitespace exceeds the DTO contract"
	);

	// Act
	CreateDeploymentFormRequestClientForm::normalize_values(&runtime);
	let normalized = CreateDeploymentFormRequestClientForm::to_request(&runtime);
	let dispatched = normalized.clone();
	let outcome = runtime
		.submit_server_fn(move || async move {
			dispatched.validate().map_err(ServerFnError::from)?;
			Ok(dispatched)
		})
		.await
		.expect("normalized request must reach the server");

	// Assert
	assert_eq!(
		outcome,
		UseFormAsyncSubmitOutcome::Submitted(normalized.clone())
	);
	assert_eq!(request.normalized(), normalized);
	assert_eq!(normalized.project_name, "a".repeat(63));
	assert_eq!(normalized.image, "i".repeat(512));
	assert_eq!(normalized.project_yaml, "  preserved manifest bytes\n");
}

#[rstest]
#[tokio::test]
async fn deployment_update_normalizes_before_client_validation_and_server_dispatch() {
	// Arrange
	let request = UpdateDeploymentFormRequest {
		deployment_id: "7".to_owned(),
		project_name: format!(" {} ", "a".repeat(63)),
		image: format!(" {} ", "i".repeat(512)),
		status: format!(" {} ", "s".repeat(50)),
	};
	let scope = ReactiveScope::new();
	let runtime = scope.enter(|| {
		let form = UpdateDeploymentFormRequestClientForm::new().with_defaults(request.clone());
		use_form(&form).build()
	});
	assert!(
		runtime.trigger().is_err(),
		"raw whitespace exceeds the DTO contract"
	);

	// Act
	UpdateDeploymentFormRequestClientForm::normalize_values(&runtime);
	let normalized = UpdateDeploymentFormRequestClientForm::to_request(&runtime);
	let dispatched = normalized.clone();
	let outcome = runtime
		.submit_server_fn(move || async move {
			dispatched.validate().map_err(ServerFnError::from)?;
			Ok(dispatched)
		})
		.await
		.expect("normalized request must reach the server");

	// Assert
	assert_eq!(
		outcome,
		UseFormAsyncSubmitOutcome::Submitted(normalized.clone())
	);
	assert_eq!(request.normalized(), normalized);
	assert_eq!(normalized.project_name, "a".repeat(63));
	assert_eq!(normalized.image, "i".repeat(512));
	assert_eq!(normalized.status, "s".repeat(50));
}

#[rstest]
#[tokio::test]
async fn deployment_status_normalizes_before_client_validation_and_server_dispatch() {
	// Arrange
	let request = UpdateDeploymentStatusFormRequest {
		deployment_id: "7".to_owned(),
		status: format!(" {} ", "s".repeat(50)),
	};
	let scope = ReactiveScope::new();
	let runtime = scope.enter(|| {
		let form =
			UpdateDeploymentStatusFormRequestClientForm::new().with_defaults(request.clone());
		use_form(&form).build()
	});
	assert!(
		runtime.trigger().is_err(),
		"raw whitespace exceeds the DTO contract"
	);

	// Act
	UpdateDeploymentStatusFormRequestClientForm::normalize_values(&runtime);
	let normalized = UpdateDeploymentStatusFormRequestClientForm::to_request(&runtime);
	let dispatched = normalized.clone();
	let outcome = runtime
		.submit_server_fn(move || async move {
			dispatched.validate().map_err(ServerFnError::from)?;
			Ok(dispatched)
		})
		.await
		.expect("normalized request must reach the server");

	// Assert
	assert_eq!(
		outcome,
		UseFormAsyncSubmitOutcome::Submitted(normalized.clone())
	);
	assert_eq!(request.normalized(), normalized);
	assert_eq!(normalized.status, "s".repeat(50));
}

#[rstest]
#[tokio::test]
async fn github_import_normalizes_before_client_validation_and_server_dispatch() {
	// Arrange
	let request = GitHubRepositoryImportRequest {
		repository_id: "2".to_owned(),
		cluster_id: "41".to_owned(),
		project_name: format!(" {} ", "a".repeat(63)),
		registry: format!(" {} ", "r".repeat(512)),
	};
	let scope = ReactiveScope::new();
	let runtime = scope.enter(|| {
		let form = GitHubRepositoryImportRequestClientForm::new().with_defaults(request.clone());
		use_form(&form).build()
	});
	assert!(
		runtime.trigger().is_err(),
		"raw whitespace exceeds the DTO contract"
	);

	// Act
	GitHubRepositoryImportRequestClientForm::normalize_values(&runtime);
	let normalized = GitHubRepositoryImportRequestClientForm::to_request(&runtime);
	let dispatched = normalized.clone();
	let outcome = runtime
		.submit_server_fn(move || async move {
			dispatched.validate().map_err(ServerFnError::from)?;
			Ok(dispatched)
		})
		.await
		.expect("normalized request must reach the server");

	// Assert
	assert_eq!(
		outcome,
		UseFormAsyncSubmitOutcome::Submitted(normalized.clone())
	);
	assert_eq!(request.normalized(), normalized);
	assert_eq!(normalized.project_name, "a".repeat(63));
	assert_eq!(normalized.registry, "r".repeat(512));
}
