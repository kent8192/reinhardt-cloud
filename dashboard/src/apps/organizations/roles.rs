//! Role definitions and slug helpers for the organizations app.
//!
//! Membership roles are stored in the database as a lower-case string. The
//! application layer parses to/from the `MembershipRole` enum. A `CHECK`
//! constraint on the database column enforces the value set (see
//! `migrations/organizations/0001_initial.rs`).
//!
//! Slug sanitization produces DNS-1123 labels suitable for both URL paths
//! and Kubernetes namespace names (see sub-issue #416).

use serde::{Deserialize, Serialize};

// =====================================================================
// MembershipRole
// =====================================================================

/// Hierarchical organization membership role.
///
/// Owner > Admin > Developer > Viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MembershipRole {
	Viewer = 0,
	Developer = 1,
	Admin = 2,
	Owner = 3,
}

impl MembershipRole {
	/// True if this role grants at least the privileges of `required`.
	pub fn can(self, required: Self) -> bool {
		self >= required
	}

	/// Parse a database-stored string. Case-insensitive. Returns `None`
	/// for unknown values; callers should treat that as a 5xx (the CHECK
	/// constraint should make this impossible at runtime).
	pub fn from_db_str(s: &str) -> Option<Self> {
		match s.to_ascii_lowercase().as_str() {
			"owner" => Some(Self::Owner),
			"admin" => Some(Self::Admin),
			"developer" => Some(Self::Developer),
			"viewer" => Some(Self::Viewer),
			_ => None,
		}
	}

	/// Lower-case canonical form for storage.
	pub fn as_db_str(self) -> &'static str {
		match self {
			Self::Owner => "owner",
			Self::Admin => "admin",
			Self::Developer => "developer",
			Self::Viewer => "viewer",
		}
	}
}

// =====================================================================
// Slug validation and sanitization (DNS-1123 label)
// =====================================================================

/// Maximum length for a DNS-1123 label (also the K8s namespace name limit).
pub const MAX_SLUG_LEN: usize = 63;

/// Slugs that collide with system namespaces or reserved URL prefixes.
const RESERVED_SLUGS: &[&str] = &[
	"",
	"kube-system",
	"kube-public",
	"kube-node-lease",
	"default",
	"reinhardt-cloud-system",
	"system",
	"admin",
	"api",
	"dashboard",
];

/// Returns true if the slug is reserved and cannot be assigned to a tenant.
pub fn is_reserved_slug(slug: &str) -> bool {
	RESERVED_SLUGS.contains(&slug)
}

/// Validate a slug as a DNS-1123 label.
///
/// Rules: lowercase, length 1..=63, must start with a letter, must end
/// with a letter or digit, may contain only `[a-z0-9-]`.
pub fn validate_slug(slug: &str) -> Result<(), &'static str> {
	if slug.is_empty() {
		return Err("slug is empty");
	}
	if slug.len() > MAX_SLUG_LEN {
		return Err("slug exceeds 63 characters");
	}
	let bytes = slug.as_bytes();
	let first = bytes[0];
	if !first.is_ascii_lowercase() {
		return Err("slug must start with a lowercase letter");
	}
	let last = *bytes.last().expect("non-empty");
	if !(last.is_ascii_lowercase() || last.is_ascii_digit()) {
		return Err("slug must end with a lowercase letter or digit");
	}
	for b in bytes {
		if !(b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-') {
			return Err("slug contains invalid characters");
		}
	}
	Ok(())
}

/// Convert an arbitrary username to a DNS-1123 compliant slug, truncating
/// to `MAX_SLUG_LEN` and ensuring leading/trailing characters are valid.
///
/// Falls back to `user-<uuid-prefix>` when the input has no usable
/// characters (e.g., all symbols) or starts with a digit.
pub fn sanitize_username_to_slug(username: &str) -> String {
	let lowered: String = username
		.chars()
		.flat_map(|c| {
			if c.is_ascii_alphanumeric() {
				vec![c.to_ascii_lowercase()]
			} else if matches!(c, '-' | '_' | '.' | ' ') {
				vec!['-']
			} else {
				vec![]
			}
		})
		.collect();

	// Collapse repeated dashes and trim from both ends.
	let mut collapsed = String::with_capacity(lowered.len());
	let mut prev_dash = true; // suppress leading dashes
	for c in lowered.chars() {
		if c == '-' {
			if prev_dash {
				continue;
			}
			collapsed.push(c);
			prev_dash = true;
		} else {
			collapsed.push(c);
			prev_dash = false;
		}
	}
	while collapsed.ends_with('-') {
		collapsed.pop();
	}

	if collapsed.len() > MAX_SLUG_LEN {
		collapsed.truncate(MAX_SLUG_LEN);
		while collapsed.ends_with('-') {
			collapsed.pop();
		}
	}

	if collapsed.is_empty() || collapsed.as_bytes()[0].is_ascii_digit() {
		// Pad with a `user-` prefix so we always start with a letter.
		let suffix = uuid::Uuid::new_v4().simple().to_string();
		let mut result = String::from("user-");
		result.push_str(&suffix[..8]);
		result
	} else {
		collapsed
	}
}
