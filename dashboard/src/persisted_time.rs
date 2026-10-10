//! Timestamps that are safe to persist.
//!
//! PostgreSQL stores `timestamptz` with microsecond precision, and the native
//! database bindings of `reinhardt-web` refuse any temporal argument with
//! sub-microsecond precision instead of rounding it. Linux clocks report
//! nanoseconds (macOS reports microseconds, which hides the failure during
//! local development), so a raw `Utc::now()` or `Utc::now() + expires_in` fails
//! on Linux at the first write.
//!
//! Every timestamp the Control Plane computes and then stores passes through
//! [`persisted_now`] or [`to_persisted`], so the truncation happens in exactly
//! one place.
//!
//! Workaround for kent8192/reinhardt-web#6703 (tracked in
//! kent8192/reinhardt-cloud#942). Remove it when the upstream issue is
//! resolved and the dashboard runs on a version with the fix.
//!
//! Ideal implementation (without workaround):
//!   `User::field_updated_at().assign(Utc::now())`
//!   The generated binding accepts the clock's own precision, exactly as it
//!   must for the `auto_now` and `auto_now_add` values the framework generates.

use chrono::{DateTime, SubsecRound, Utc};

/// The current time, truncated to the precision PostgreSQL stores.
#[must_use]
pub fn persisted_now() -> DateTime<Utc> {
	to_persisted(Utc::now())
}

/// Truncate `timestamp` to microseconds, the precision PostgreSQL stores.
///
/// Truncation (not rounding) never moves an expiry later than the provider
/// reported it.
#[must_use]
pub fn to_persisted(timestamp: DateTime<Utc>) -> DateTime<Utc> {
	timestamp.trunc_subsecs(6)
}

#[cfg(test)]
mod tests {
	use chrono::{DateTime, Timelike, Utc};
	use rstest::rstest;

	use super::{persisted_now, to_persisted};

	fn with_nanoseconds(nanoseconds: u32) -> DateTime<Utc> {
		DateTime::<Utc>::from_timestamp(1_700_000_000, nanoseconds)
			.expect("timestamp should be in range")
	}

	#[rstest]
	#[case::nanoseconds(123_456_789, 123_456_000)]
	#[case::single_nanosecond(1, 0)]
	#[case::just_below_a_microsecond(999, 0)]
	#[case::whole_microseconds_unchanged(123_456_000, 123_456_000)]
	#[case::whole_seconds_unchanged(0, 0)]
	#[case::truncates_not_rounds(999_999_999, 999_999_000)]
	fn to_persisted_truncates_to_microseconds(#[case] nanoseconds: u32, #[case] expected: u32) {
		// Arrange
		let timestamp = with_nanoseconds(nanoseconds);

		// Act
		let persisted = to_persisted(timestamp);

		// Assert
		assert_eq!(persisted, with_nanoseconds(expected));
		assert_eq!(persisted.nanosecond() % 1_000, 0);
		assert!(persisted <= timestamp);
	}

	#[rstest]
	fn persisted_now_has_no_sub_microsecond_part() {
		// Arrange / Act
		let now = persisted_now();

		// Assert
		assert_eq!(now.nanosecond() % 1_000, 0);
	}
}
