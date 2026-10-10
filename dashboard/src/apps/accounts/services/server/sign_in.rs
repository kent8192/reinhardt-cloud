//! The GitHub sign-in callback, from provider response to browser session.
//!
//! [`SignInService::complete`] is the one place that turns what GitHub sent
//! into a signed-in User, in this order:
//!
//! 1. the callback's state is consumed (single use) and checked against the
//!    provider and the browser binding (SR-04);
//! 2. GitHub's profile is read; a failed `/user` call is a failed sign-in;
//! 3. the User is found by numeric GitHub ID, or created only if the sign-up
//!    policy admits the account (SR-02, SR-19), before any token or session is
//!    stored;
//! 4. an inactive User is turned away (SR-07);
//! 5. the profile is synchronized, the provider tokens are stored encrypted
//!    (SR-06), `last_login` is recorded, and a new session is issued while the
//!    session the browser presented, if any, is destroyed (SR-08).
//!
//! Every outcome is audited with the shared event shape, and no outcome
//! carries provider text.

use std::sync::Arc;

use reinhardt::conf::settings::secret_types::SecretString;
use reinhardt::db::orm::Model;

use crate::apps::accounts::models::User;
use crate::apps::accounts::services::server::github::{CompleteError, GithubSignIn};
use crate::apps::accounts::services::server::provider_tokens::OrmSocialAccountStorage;
use crate::apps::accounts::services::server::sessions::{
	IssuedSession, SessionService, SessionToken,
};
use crate::apps::accounts::services::server::sign_up_policy::SignUpPolicy;
use crate::apps::accounts::services::server::users::{
	FirstSignInError, ResolvedUser, resolve_first_sign_in, sync_profile,
};
use crate::audit::{ActorKind, AuditEvent, Outcome};
use crate::persisted_time::persisted_now;

/// What the browser sent to the callback.
#[derive(Debug)]
pub struct CallbackRequest<'a> {
	/// The `code` query parameter, absent when GitHub reported an error or the
	/// visitor declined.
	pub code: Option<&'a str>,
	/// The `state` query parameter.
	pub state: Option<&'a str>,
	/// The binding cookie.
	pub binding: Option<&'a str>,
	/// The session cookie, if any; it is destroyed when sign-in succeeds.
	pub previous_session: Option<SessionToken>,
}

/// How a sign-in attempt ended.
#[derive(Debug)]
pub enum SignInOutcome {
	/// The User is signed in; set the session cookie.
	SignedIn {
		/// The new session.
		session: IssuedSession,
	},
	/// The sign-up policy did not admit the account.
	NotInvited {
		/// GitHub login of the account, to address the visitor by name.
		login: String,
	},
	/// The attempt failed for a reason the visitor is not told.
	Failed,
}

/// Runs sign-in callbacks.
#[derive(Clone)]
pub struct SignInService {
	github: GithubSignIn,
	sessions: SessionService,
	tokens: Arc<OrmSocialAccountStorage>,
	policy: SignUpPolicy,
}

impl std::fmt::Debug for SignInService {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("SignInService").finish_non_exhaustive()
	}
}

impl SignInService {
	/// Assemble the service.
	#[must_use]
	pub fn new(
		github: GithubSignIn,
		sessions: SessionService,
		tokens: Arc<OrmSocialAccountStorage>,
		policy: SignUpPolicy,
	) -> Self {
		Self {
			github,
			sessions,
			tokens,
			policy,
		}
	}

	/// The GitHub flow, for starting a sign-in.
	#[must_use]
	pub fn github(&self) -> &GithubSignIn {
		&self.github
	}

	/// Handle the GitHub callback.
	pub async fn complete(&self, request: CallbackRequest<'_>) -> SignInOutcome {
		let (Some(code), Some(state)) = (request.code, request.state) else {
			// The visitor declined or GitHub reported an error. The state, if
			// any, is still used up so it cannot be replayed.
			if let Some(state) = request.state {
				self.github.discard_state(state).await;
			}
			return fail(CompleteError::InvalidState.code());
		};

		let identity = match self.github.complete(code, state, request.binding).await {
			Ok(identity) => identity,
			Err(error) => return fail(error.code()),
		};

		let membership = self.github.memberships(SecretString::new(
			identity.tokens.access_token.expose_secret(),
		));
		let resolved =
			match resolve_first_sign_in(&identity.profile, &self.policy, &membership).await {
				Ok(resolved) => resolved,
				Err(FirstSignInError::Denied(reason)) => {
					// `accounts.sign_up.denied` was already audited by the policy.
					AuditEvent::new("accounts.sign_in.denied", ActorKind::User, Outcome::Denied)
						.github_user(identity.profile.github_user_id)
						.reason(reason.code())
						.emit();
					return SignInOutcome::NotInvited {
						login: identity.profile.login,
					};
				}
				Err(FirstSignInError::User(error)) => {
					tracing::error!(%error, "user lookup failed during sign-in");
					return fail("internal");
				}
			};

		let user = match resolved {
			ResolvedUser::Existing(user) => user,
			ResolvedUser::Created(user) => user,
		};
		if !user.is_active {
			AuditEvent::new("accounts.sign_in.denied", ActorKind::User, Outcome::Denied)
				.subject_user(user.id)
				.github_user(user.github_user_id)
				.reason("deactivated")
				.emit();
			return SignInOutcome::Failed;
		}

		match self.finish(user, &identity, request.previous_session).await {
			Ok(outcome) => outcome,
			Err(code) => fail(code),
		}
	}

	async fn finish(
		&self,
		user: User,
		identity: &crate::apps::accounts::services::server::github::GithubIdentity,
		previous_session: Option<SessionToken>,
	) -> Result<SignInOutcome, &'static str> {
		let user = sync_profile(user, &identity.profile)
			.await
			.map_err(|error| {
				tracing::error!(%error, "profile synchronization failed during sign-in");
				"internal"
			})?;
		self.tokens
			.store_tokens(user.id, &identity.tokens)
			.await
			.map_err(|error| {
				tracing::error!(%error, "provider token storage failed during sign-in");
				"internal"
			})?;
		record_last_login(user.id).await.map_err(|error| {
			tracing::error!(%error, "recording the last login failed");
			"internal"
		})?;

		// Rotation (SR-08): the browser gets a new token, and the one it
		// presented stops working.
		if let Some(previous) = previous_session {
			let _ = self.sessions.destroy(&previous).await;
		}
		let session = self.sessions.create(user.id).await.map_err(|error| {
			tracing::error!(%error, "session creation failed during sign-in");
			"internal"
		})?;

		AuditEvent::new(
			"accounts.sign_in.succeeded",
			ActorKind::User,
			Outcome::Succeeded,
		)
		.actor_user(user.id)
		.subject_user(user.id)
		.github_user(user.github_user_id)
		.emit();
		Ok(SignInOutcome::SignedIn { session })
	}
}

fn fail(reason: &'static str) -> SignInOutcome {
	AuditEvent::new("accounts.sign_in.failed", ActorKind::User, Outcome::Failed)
		.reason(reason)
		.emit();
	SignInOutcome::Failed
}

/// Stamp `last_login` without touching any other column.
async fn record_last_login(user_id: uuid::Uuid) -> Result<(), String> {
	let now = persisted_now();
	let updated = User::objects()
		.filter(User::field_id().eq(user_id))
		.update_fields([
			User::field_last_login().assign(Some(now)),
			User::field_updated_at().assign(now),
		])
		.await
		.map_err(|error| error.to_string())?;
	if updated == 1 {
		Ok(())
	} else {
		Err("user disappeared during sign-in".to_owned())
	}
}
