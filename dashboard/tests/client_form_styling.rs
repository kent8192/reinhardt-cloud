//! The design's class values applied to a `form!` rendered from a `ClientForm`.

use cloud_control_plane::components::form_styles::form_classes;
use reinhardt::pages::component::{Page, PageElement};
use reinhardt::pages::reactive::ReactiveScope;
use reinhardt::pages::server_fn::{ServerFnError, server_fn};
use reinhardt::pages::{ClientFormChoices, client_form, form, use_form};
use rstest::rstest;
use serde::{Deserialize, Serialize};

#[client_form(server_fn = submit_cluster_name)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClusterNameRequest {
	pub name: String,
}

#[server_fn]
pub async fn submit_cluster_name(request: ClusterNameRequest) -> Result<(), ServerFnError> {
	let _ = request;
	Ok(())
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ClientFormChoices)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationMode {
	#[default]
	Manual,
	Automatic,
}

#[client_form(server_fn = submit_cluster_mode)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClusterModeRequest {
	pub mode: RegistrationMode,
}

#[server_fn]
pub async fn submit_cluster_mode(request: ClusterModeRequest) -> Result<(), ServerFnError> {
	let _ = request;
	Ok(())
}

fn elements<'a>(page: &'a Page, tag: &str) -> Vec<&'a PageElement> {
	let mut result = Vec::new();
	if let Page::Element(element) = page {
		if element.tag_name() == tag {
			result.push(element);
		}
		for child in element.child_views() {
			result.extend(elements(child, tag));
		}
	}
	result
}

fn attr<'a>(element: &'a PageElement, name: &str) -> Option<&'a str> {
	element
		.attrs()
		.iter()
		.find(|(key, _)| key == name)
		.map(|(_, value)| value.as_ref())
}

#[rstest]
fn client_form_renders_the_design_classes() {
	ReactiveScope::run(|| {
		// Arrange
		let classes = form_classes();
		let definition = ClusterNameRequestClientForm::new();
		let runtime = use_form(&definition).build();
		let mutation = definition.server_mutation(&runtime).build();

		// Act
		let page = form! {
			client_form: ClusterNameRequestClientForm,
			mutation: &mutation,
			id: "cluster-name",
			styling: {
				class: classes.form.clone(),
				field_class: classes.field.clone(),
				input_class: classes.input.clone(),
				label_class: classes.label.clone(),
				help_class: classes.help.clone(),
				error_class: classes.error.clone(),
				summary_class: classes.summary.clone(),
			},
			submit: {
				label: "Create",
				class: classes.submit.clone()
			},
		}
		.into_page();

		// Assert
		assert_eq!(
			attr(elements(&page, "form")[0], "class"),
			Some(classes.form.as_str())
		);
		assert_eq!(
			attr(elements(&page, "input")[0], "class"),
			Some(classes.input.as_str())
		);
		assert_eq!(
			attr(elements(&page, "label")[0], "class"),
			Some(classes.label.as_str())
		);
		assert_eq!(
			attr(elements(&page, "button")[0], "class"),
			Some(classes.submit.as_str())
		);
	});
}

#[rstest]
fn client_form_select_takes_the_select_class_through_customize() {
	ReactiveScope::run(|| {
		// Arrange
		let classes = form_classes();
		let definition = ClusterModeRequestClientForm::new();
		let runtime = use_form(&definition).build();
		let mutation = definition.server_mutation(&runtime).build();

		// Act
		let page = form! {
			client_form: ClusterModeRequestClientForm,
			mutation: &mutation,
			id: "cluster-mode",
			styling: {
				input_class: classes.input.clone()
			},
			customize: {
				mode: {
					class: classes.select.clone()
				}
			},
		}
		.into_page();

		// Assert
		assert_eq!(elements(&page, "input").len(), 0);
		assert_eq!(
			attr(elements(&page, "select")[0], "class"),
			Some(classes.select.as_str())
		);
	});
}
