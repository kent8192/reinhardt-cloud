//! Shared entity selection controls for dashboard forms.

use reinhardt::pages::component::{IntoPage, Page, PageElement};
use reinhardt::pages::page;
use reinhardt::pages::prelude::Signal;

/// Display option for selecting a persisted dashboard entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntitySelectOption {
	pub value: String,
	pub label: String,
	pub detail: Option<String>,
}

impl EntitySelectOption {
	/// Build an option with an optional detail line.
	pub fn new(value: impl Into<String>, label: impl Into<String>, detail: Option<String>) -> Self {
		Self {
			value: value.into(),
			label: label.into(),
			detail,
		}
	}

	fn display_label(&self) -> String {
		match &self.detail {
			Some(detail) if !detail.is_empty() => format!("{} - {}", self.label, detail),
			_ => self.label.clone(),
		}
	}
}

#[cfg(wasm)]
fn select_change_handler<F>(
	selected_value: Signal<String>,
	on_change: F,
) -> impl Fn(web_sys::Event) + 'static
where
	F: Fn(String) + 'static,
{
	use wasm_bindgen::JsCast;

	move |event| {
		let Some(target) = event.target() else {
			return;
		};
		let Ok(select) = target.dyn_into::<web_sys::HtmlSelectElement>() else {
			return;
		};
		let value = select.value();
		selected_value.set(value.clone());
		on_change(value);
	}
}

#[cfg(native)]
fn select_change_handler<F>(
	_selected_value: Signal<String>,
	_on_change: F,
) -> impl Fn(reinhardt::pages::component::DummyEvent) + 'static
where
	F: Fn(String) + 'static,
{
	|_event| {}
}

/// Render a labeled select that writes the selected entity ID into a form signal.
pub fn entity_select<F>(
	label: &str,
	placeholder: &str,
	options: Vec<EntitySelectOption>,
	selected_value: Signal<String>,
	on_change: F,
) -> Page
where
	F: Fn(String) + 'static,
{
	let current_value = selected_value.get();
	let is_empty = options.is_empty();
	let placeholder_option = PageElement::new("option")
		.attr("value", "")
		.bool_attr("selected", current_value.is_empty())
		.bool_attr("disabled", !current_value.is_empty())
		.child(if is_empty {
			"No options available".to_string()
		} else {
			placeholder.to_string()
		})
		.into_page();
	let option_pages = options
		.into_iter()
		.map(|option| {
			PageElement::new("option")
				.attr("value", option.value.clone())
				.bool_attr("selected", option.value == current_value)
				.child(option.display_label())
				.into_page()
		})
		.collect::<Vec<_>>();
	let select = PageElement::new("select")
		.attr("class", "rc-input")
		.bool_attr("disabled", is_empty)
		.listener("change", select_change_handler(selected_value, on_change))
		.child(placeholder_option)
		.children(option_pages)
		.into_page();

	page!(|label: String, select: Page| {
		div {
			label { { label } }
			{ select }
		}
	})(label.to_string(), select)
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	#[case(None, "api", "api")]
	#[case(Some("active".to_string()), "api", "api - active")]
	fn test_entity_select_option_display_label(
		#[case] detail: Option<String>,
		#[case] label: &str,
		#[case] expected: &str,
	) {
		// Arrange
		let option = EntitySelectOption::new("1", label, detail);

		// Act
		let actual = option.display_label();

		// Assert
		assert_eq!(actual, expected);
	}
}
