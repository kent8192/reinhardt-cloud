//! The shared audit-event helper.
//!
//! Every security-relevant decision in the Control Plane is recorded through
//! [`AuditEvent`], so all applications emit one shape: a `tracing` event at
//! `info` level with target [`AUDIT_TARGET`] and the fields below. Sinks select
//! audit records by that target.
//!
//! | Field | Meaning |
//! |-------|---------|
//! | `event` | Dotted event name, for example `accounts.sign_in.succeeded` |
//! | `actor_kind` | `user`, `host_operator`, or `system` |
//! | `actor_user_id` | Internal ID of the acting User, when there is one |
//! | `subject_user_id` | Internal ID of the User acted upon, when there is one |
//! | `github_user_id` | Numeric GitHub user ID, when relevant |
//! | `outcome` | `succeeded`, `denied`, or `failed` |
//! | `reason` | Short static code explaining a denial or failure |
//!
//! The type deliberately has no field for free text, tokens, codes, cookies,
//! secrets, or email addresses: `reason` is a `&'static str`, so a value
//! computed from request data cannot be recorded.

use uuid::Uuid;

/// The `tracing` target of every audit event.
pub const AUDIT_TARGET: &str = "audit";

/// Who performed the audited action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorKind {
	/// A signed-in or signing-in person.
	User,
	/// Someone with operator access to the Control Plane's host (`manage`).
	HostOperator,
	/// The Control Plane itself, for example a scheduled cleanup.
	System,
}

impl ActorKind {
	/// The field value written to the audit record.
	#[must_use]
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::User => "user",
			Self::HostOperator => "host_operator",
			Self::System => "system",
		}
	}
}

/// How the audited action ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
	/// The action was carried out.
	Succeeded,
	/// The action was refused by policy or authorization.
	Denied,
	/// The action was attempted and an error stopped it.
	Failed,
}

impl Outcome {
	/// The field value written to the audit record.
	#[must_use]
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Succeeded => "succeeded",
			Self::Denied => "denied",
			Self::Failed => "failed",
		}
	}
}

/// One audit record. Build it, then call [`AuditEvent::emit`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[must_use = "an audit event is only recorded when it is emitted"]
pub struct AuditEvent {
	event: &'static str,
	actor_kind: ActorKind,
	outcome: Outcome,
	actor_user_id: Option<Uuid>,
	subject_user_id: Option<Uuid>,
	github_user_id: Option<i64>,
	reason: Option<&'static str>,
}

impl AuditEvent {
	/// Start a record. `event` is a dotted name such as `accounts.sign_in.denied`.
	pub const fn new(event: &'static str, actor_kind: ActorKind, outcome: Outcome) -> Self {
		Self {
			event,
			actor_kind,
			outcome,
			actor_user_id: None,
			subject_user_id: None,
			github_user_id: None,
			reason: None,
		}
	}

	/// Record the acting User.
	pub const fn actor_user(mut self, id: Uuid) -> Self {
		self.actor_user_id = Some(id);
		self
	}

	/// Record the User acted upon.
	pub const fn subject_user(mut self, id: Uuid) -> Self {
		self.subject_user_id = Some(id);
		self
	}

	/// Record the numeric GitHub user ID involved.
	pub const fn github_user(mut self, id: i64) -> Self {
		self.github_user_id = Some(id);
		self
	}

	/// Record a short, static reason code.
	pub const fn reason(mut self, code: &'static str) -> Self {
		self.reason = Some(code);
		self
	}

	/// Write the record to the `audit` target.
	pub fn emit(&self) {
		// `Option<String>` records nothing for `None`, so absent fields are
		// omitted rather than written as empty values.
		tracing::info!(
			target: "audit",
			event = self.event,
			actor_kind = self.actor_kind.as_str(),
			actor_user_id = self.actor_user_id.map(|id| id.to_string()),
			subject_user_id = self.subject_user_id.map(|id| id.to_string()),
			github_user_id = self.github_user_id,
			outcome = self.outcome.as_str(),
			reason = self.reason,
		);
	}
}

/// Capture of `audit` events for tests.
#[cfg(test)]
pub(crate) mod capture {
	use std::collections::BTreeMap;
	use std::fmt;
	use std::future::Future;
	use std::sync::{Arc, Mutex};

	use tracing::field::{Field, Visit};
	use tracing::{Event, Level, Subscriber};
	use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
	use tracing_subscriber::registry::Registry;

	/// One recorded `audit` event.
	#[derive(Clone, Debug)]
	pub(crate) struct CapturedEvent {
		pub(crate) level: Level,
		fields: BTreeMap<String, String>,
	}

	impl CapturedEvent {
		/// The recorded value of `name`, if the event carried that field.
		pub(crate) fn field(&self, name: &str) -> Option<&str> {
			self.fields.get(name).map(String::as_str)
		}

		/// Names of all recorded fields, sorted.
		pub(crate) fn field_names(&self) -> Vec<&str> {
			self.fields.keys().map(String::as_str).collect()
		}
	}

	#[derive(Default)]
	struct FieldCollector(BTreeMap<String, String>);

	impl Visit for FieldCollector {
		fn record_str(&mut self, field: &Field, value: &str) {
			self.0.insert(field.name().to_owned(), value.to_owned());
		}

		fn record_i64(&mut self, field: &Field, value: i64) {
			self.0.insert(field.name().to_owned(), value.to_string());
		}

		fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
			self.0.insert(field.name().to_owned(), format!("{value:?}"));
		}
	}

	struct CaptureLayer(Arc<Mutex<Vec<CapturedEvent>>>);

	impl<S: Subscriber> Layer<S> for CaptureLayer {
		fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
			if event.metadata().target() != super::AUDIT_TARGET {
				return;
			}
			let mut collector = FieldCollector::default();
			event.record(&mut collector);
			self.0
				.lock()
				.expect("capture lock should not be poisoned")
				.push(CapturedEvent {
					level: *event.metadata().level(),
					fields: collector.0,
				});
		}
	}

	/// Run `future` and return the `audit` events emitted on this thread while
	/// it ran, together with its output.
	///
	/// The subscriber is thread-local, so use it on a current-thread runtime
	/// (the default of `#[tokio::test]`); events from spawned tasks are not
	/// captured.
	pub(crate) async fn capture_audit_events<F: Future>(
		future: F,
	) -> (Vec<CapturedEvent>, F::Output) {
		let events = Arc::new(Mutex::new(Vec::new()));
		let subscriber = Registry::default().with(CaptureLayer(Arc::clone(&events)));
		let guard = tracing::subscriber::set_default(subscriber);
		let output = future.await;
		drop(guard);
		let captured = events
			.lock()
			.expect("capture lock should not be poisoned")
			.clone();
		(captured, output)
	}
}

#[cfg(test)]
mod tests {
	use rstest::rstest;
	use uuid::Uuid;

	use super::capture::capture_audit_events;
	use super::{ActorKind, AuditEvent, Outcome};

	#[rstest]
	#[tokio::test]
	async fn sr_102_audit_event_has_the_shared_shape() {
		// Arrange
		let actor = Uuid::now_v7();
		let subject = Uuid::now_v7();
		let event = AuditEvent::new("accounts.sign_in.denied", ActorKind::User, Outcome::Denied)
			.actor_user(actor)
			.subject_user(subject)
			.github_user(42)
			.reason("deactivated");

		// Act
		let (events, ()) = capture_audit_events(async { event.emit() }).await;

		// Assert
		assert_eq!(events.len(), 1);
		let recorded = &events[0];
		assert_eq!(recorded.level, tracing::Level::INFO);
		assert_eq!(
			recorded.field_names(),
			vec![
				"actor_kind",
				"actor_user_id",
				"event",
				"github_user_id",
				"outcome",
				"reason",
				"subject_user_id"
			]
		);
		assert_eq!(recorded.field("event"), Some("accounts.sign_in.denied"));
		assert_eq!(recorded.field("actor_kind"), Some("user"));
		assert_eq!(recorded.field("outcome"), Some("denied"));
		assert_eq!(recorded.field("reason"), Some("deactivated"));
		assert_eq!(recorded.field("github_user_id"), Some("42"));
		assert_eq!(
			recorded.field("actor_user_id"),
			Some(actor.to_string().as_str())
		);
		assert_eq!(
			recorded.field("subject_user_id"),
			Some(subject.to_string().as_str())
		);
	}

	#[rstest]
	#[tokio::test]
	async fn audit_event_omits_absent_optional_fields() {
		// Arrange
		let event = AuditEvent::new(
			"accounts.grant_staff.succeeded",
			ActorKind::HostOperator,
			Outcome::Succeeded,
		);

		// Act
		let (events, ()) = capture_audit_events(async { event.emit() }).await;

		// Assert
		assert_eq!(
			events[0].field_names(),
			vec!["actor_kind", "event", "outcome"]
		);
		assert_eq!(events[0].field("actor_kind"), Some("host_operator"));
		assert_eq!(events[0].field("outcome"), Some("succeeded"));
	}

	#[rstest]
	#[case(ActorKind::User, "user")]
	#[case(ActorKind::HostOperator, "host_operator")]
	#[case(ActorKind::System, "system")]
	fn actor_kinds_use_the_documented_names(#[case] kind: ActorKind, #[case] name: &str) {
		// Arrange / Act / Assert
		assert_eq!(kind.as_str(), name);
	}

	#[rstest]
	#[case(Outcome::Succeeded, "succeeded")]
	#[case(Outcome::Denied, "denied")]
	#[case(Outcome::Failed, "failed")]
	fn outcomes_use_the_documented_names(#[case] outcome: Outcome, #[case] name: &str) {
		// Arrange / Act / Assert
		assert_eq!(outcome.as_str(), name);
	}

	#[rstest]
	#[tokio::test]
	async fn sr_102_audit_events_are_written_to_the_audit_target_only() {
		// Arrange
		let event = AuditEvent::new(
			"accounts.sign_out.succeeded",
			ActorKind::User,
			Outcome::Succeeded,
		);

		// Act
		let (events, ()) = capture_audit_events(async {
			tracing::info!(target: "not_audit", event = "noise");
			event.emit();
		})
		.await;

		// Assert
		assert_eq!(events.len(), 1);
		assert_eq!(
			events[0].field("event"),
			Some("accounts.sign_out.succeeded")
		);
	}
}
