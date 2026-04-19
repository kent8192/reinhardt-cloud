//! W3C traceparent helpers for cross-process trace-context propagation.

use std::collections::HashMap;

use opentelemetry::propagation::{Extractor, Injector};
use opentelemetry::{Context, global};

/// Parse a W3C `traceparent` string into an OTel [`Context`].
pub fn context_from_traceparent(traceparent: &str) -> Context {
	let carrier = MapCarrier([("traceparent".to_string(), traceparent.to_string())].into());
	global::get_text_map_propagator(|p| p.extract(&carrier))
}

/// Serialize the given [`Context`] to a W3C `traceparent` string, or `None`
/// when the context has no valid span.
pub fn traceparent_from_context(cx: &Context) -> Option<String> {
	let mut carrier = MapCarrier(HashMap::new());
	global::get_text_map_propagator(|p| p.inject_context(cx, &mut carrier));
	carrier.0.remove("traceparent")
}

struct MapCarrier(HashMap<String, String>);

impl Extractor for MapCarrier {
	fn get(&self, key: &str) -> Option<&str> {
		self.0.get(key).map(String::as_str)
	}

	fn keys(&self) -> Vec<&str> {
		self.0.keys().map(String::as_str).collect()
	}
}

impl Injector for MapCarrier {
	fn set(&mut self, key: &str, value: String) {
		self.0.insert(key.to_string(), value);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use opentelemetry_sdk::propagation::TraceContextPropagator;
	use rstest::rstest;
	use serial_test::serial;

	fn ensure_propagator() {
		opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
	}

	#[rstest]
	#[serial(global_propagator)]
	fn roundtrip_of_valid_traceparent_preserves_trace_id() {
		// Arrange
		ensure_propagator();
		let tp = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

		// Act
		let ctx = context_from_traceparent(tp);
		let back = traceparent_from_context(&ctx);

		// Assert
		let back = back.expect("valid traceparent must roundtrip");
		assert!(back.contains("4bf92f3577b34da6a3ce929d0e0e4736"));
	}

	#[rstest]
	#[serial(global_propagator)]
	fn empty_context_yields_no_traceparent() {
		// Arrange
		ensure_propagator();

		// Act
		let result = traceparent_from_context(&Context::new());

		// Assert
		assert!(result.is_none());
	}
}
