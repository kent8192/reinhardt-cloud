# Control Plane Security Requirements

This document restates every security property the previous Control Plane enforced as an implementation-neutral requirement, and adds the requirements introduced by the rebuild. It exists so that the properties survive the rebuild without inheriting the code that enforced them.

Vocabulary follows [`CONTEXT.md`](../../CONTEXT.md): Control Plane, Dashboard, Agent Gateway, User, Member, Invitation, Staff, API Key, Login Link, Organization, Cluster, Agent, Project, Deployment, and Preview are used exactly as defined there. The rebuild itself is described by the Control Plane rebuild plan tracked in #915, with one issue per milestone (#916 to #923).

## Scope

The requirements cover what the Control Plane (the `dashboard/` crate and the Agent Gateway it hosts) must guarantee, plus the client-side and agent-side behavior it depends on:

- Browser sign-in, sessions, and the web surface (headers, origins, errors, health).
- API Keys and the CLI submission path, including the CLI's own credential handling.
- Role-based access, Organization isolation, and the tenant namespace rule.
- The Agent Gateway and Cluster credentials, including the Agent's side of the contract.
- Logs and realtime delivery.
- GitHub App integration.
- Build and supply-chain properties of the Control Plane image and its deployment workflows.
- Configuration and secrets.

Fixes that live entirely in the operator, Terraform generation, Helm charts, or repository CI are not owned by the Control Plane. They are listed by pull request number in [Appendix B](#appendix-b-fixes-outside-the-control-plane) so the sweep is visibly complete.

Requirements are derived from the state of `origin/main` at `1c18332c0`. Source references are pull request numbers (`#N`) or commit SHAs; a SHA is given in addition to the number when a security fix landed inside a larger pull request. Duplicate pull requests for the same fix are cited once, using the one that carried the change.

## Record format

Each requirement is a record with these fields:

| Field | Meaning |
|-------|---------|
| ID | Stable identifier `SR-NN`. IDs are never reused or renumbered. |
| Status | One of the values below. |
| Milestone | The milestone that owns the acceptance test (M1 to M7). |
| Threat | What an attacker gains if the property does not hold. |
| Requirement | What must hold, stated without naming a mechanism. |
| Source | Pull requests and commits where the property was introduced or fixed. For `new` requirements, the rebuild decision that introduces it, cited by epic or milestone issue. |
| Old tests | Tests on `origin/main` that proved the property. `gap` means no test proved it. |

### Status values

| Status | Meaning |
|--------|---------|
| `carried` | The old Control Plane enforced this property and the rebuilt one must too. |
| `new` | Introduced by the rebuild plan (#915); no old behavior to port. |
| `superseded` | The old mechanism is replaced; the record names the replacing requirement. |
| `obsolete` | The surface the property protected no longer exists; the record says why. |
| `needs decision` | The status cannot be determined from the rebuild plan (#915) and the old code; the record states the question. |

A `carried` requirement whose enforcing code lives in a crate the rebuild does not touch (for example the CLI) is marked `carried (code unchanged)`. The owning milestone then re-verifies it rather than re-implementing it.

### Milestones

| Milestone | Applications |
|-----------|--------------|
| M1 | `accounts`, `health` |
| M2 | `organizations` |
| M3 | `clusters`, `agents` |
| M4 | `projects`, `deployments` |
| M5 | `logs`, realtime |
| M6 | `github` |
| M7 | Integration, CI, and end-to-end verification |

Paths in "Old tests" are relative to `dashboard/src/apps/` unless they start with `crates/`, `tests/`, or `dashboard/`. A test is written `file::function`.

## How to use

1. Each milestone owns the requirements whose Milestone field names it. A milestone is not complete until every `carried` and `new` requirement it owns has an acceptance test that passes.
2. Acceptance tests carry the requirement ID in the test function name, for example `sr_36_cross_organization_identifier_is_not_found`, so coverage can be checked with `rg 'sr_NN_'`. A requirement with several behaviors has several tests sharing the prefix.
3. Where "Old tests" lists a test, the milestone ports it to the rebuilt code and keeps its assertions. Where it says `gap`, the milestone writes the test; a requirement without a proving test is not considered met.
4. Enforcement points that were once covered only by unit tests of a helper (a permission table, an interceptor with a hand-inserted request extension) must be exercised through the real entry point: a real HTTP request, a real gRPC listener, or a real WebSocket handshake. A stale service-path prefix once disabled agent authentication while the helper's unit tests stayed green (#887).
5. A requirement marked `needs decision` blocks the owning milestone from closing it. The milestone raises the question before implementing the affected area, records the answer by changing the record's status, and then writes the acceptance test.
6. At each milestone boundary, when `main` is merged into the integration branch, every `security`-type change merged to `main` since the previous boundary is checked against this list. A change that enforces a property not yet listed adds a new record at the end of its area with the next free ID (IDs need not be contiguous within an area).
7. M7 verifies that every `carried` and `new` requirement has at least one passing acceptance test and that every `superseded` record's replacement has one. The summary table below is the checklist.

## Summary

| ID | Requirement | Status | Milestone |
|----|-------------|--------|-----------|
| SR-01 | GitHub is the only browser identity provider | `new` | M1 |
| SR-02 | A User is identified by the numeric GitHub user ID | `new` | M1 |
| SR-03 | One GitHub identity maps to exactly one User | `carried` | M1 |
| SR-04 | Sign-in flow state is single-use, expiring, provider-bound, and browser-bound | `carried` | M1 |
| SR-05 | Sign-in never attaches an identity to a session it did not start | `needs decision` | M1 |
| SR-06 | Provider access tokens are encrypted at rest | `carried` | M1 |
| SR-07 | Sessions are revalidated against the current User | `carried` | M1 |
| SR-08 | Session cookies are hardened, bounded in lifetime, and destroyed on sign-out | `carried` | M1 |
| SR-09 | Private pages and server endpoints require an authenticated session | `carried` | M1 |
| SR-10 | The unauthenticated surface is enumerated | `carried` | M1 |
| SR-11 | API documentation exposure in deployed profiles | `needs decision` | M1 |
| SR-12 | Cross-site request protection for cookie-authenticated requests | `carried` | M1 |
| SR-13 | Security response headers | `carried` | M1 |
| SR-14 | Internal errors do not reach clients | `carried` | M1 |
| SR-15 | The health endpoint is cheap and exposes only coarse status | `carried` | M1 (database probe), M3 (Agent Gateway probe) |
| SR-16 | A Login Link can be used once | `new` | M1 |
| SR-17 | A Login Link has a short lifetime | `new` | M1 |
| SR-18 | Login Links are issued only by someone with host operator access and are stored hashed | `new` | M1 |
| SR-19 | The sign-up policy is enforced at first sign-in | `new` | M1 |
| SR-20 | Staff is granted only by host operator access | `new` | M1 |
| SR-21 | Password credential handling | `obsolete` | M1 |
| SR-22 | Email verification and email change | `obsolete` | M1 |
| SR-23 | Password reset tokens | `obsolete` | M1 |
| SR-24 | Registration gating and unverified-account state | `obsolete` | M1 |
| SR-25 | Seeded default password | `obsolete` | M1 |
| SR-26 | Merging Users by verified email | `superseded` | M1 |
| SR-27 | API Keys are unguessable, recognizable, and never stored in clear | `carried` | M1 |
| SR-28 | API Key verification rejects every invalid state | `carried` | M1 |
| SR-29 | Bearer authentication never erases or elevates a session | `carried` | M1 |
| SR-30 | Usage bookkeeping cannot revive or delay anything | `carried` | M1 |
| SR-31 | Users issue and revoke their own API Keys from the Dashboard | `new` | M1 |
| SR-32 | Operator commands manage API Keys | `carried` | M1 |
| SR-33 | Authority carried by an API Key | `needs decision` | M1 |
| SR-34 | Every action is explicitly allowed or denied for every Role | `carried` | M2 |
| SR-35 | Mutations authorize by Role, in the Organization that owns the target | `carried` | M2 (rule), M3, M4, M6 (enforcement in `clusters`, `deployments`/`projects`, `github`) |
| SR-36 | Organization-owned data is isolated | `carried` | M2 (rule), M3, M4, M5, M6 (per application) |
| SR-37 | Responses do not reveal which Organizations exist | `carried` | M2 |
| SR-38 | Clients cannot set server-owned fields | `carried` | M3 (`clusters`), M4 (`projects`, `deployments`) |
| SR-39 | The tenant namespace is derived on the server and slugs are safe | `carried` | M2 |
| SR-40 | Slug length and the tenant namespace limit | `needs decision` | M2 |
| SR-41 | Submissions cannot target a namespace outside the Organization's tenant namespace | `needs decision` | M4 |
| SR-42 | An unknown stored Role fails closed | `carried` | M2 |
| SR-43 | Owner protection and no Role escalation | `new` | M2 |
| SR-44 | Project permissions | `new` | M4 |
| SR-45 | An Invitation is bound to a GitHub user ID | `new` | M2 |
| SR-46 | Invitation lifecycle | `new` | M2 |
| SR-47 | Staff authority and Organization data | `needs decision` | M2 |
| SR-48 | Cluster client credentials are shown once and stored hashed | `new` | M3 |
| SR-49 | Agent access tokens are short-lived | `new` | M3 |
| SR-50 | Revocation and rotation take effect through the OAuth store | `new` | M3 |
| SR-51 | Revocation latency is bounded | `needs decision` | M3 |
| SR-52 | Stateful credential revocation for the old per-Cluster JWT | `superseded` | M3 |
| SR-53 | Issuance of a 30-day HS256 Agent JWT per Cluster | `superseded` | M3 |
| SR-54 | Every Agent Gateway call except the standard health check requires an Agent credential | `carried` | M3 |
| SR-55 | An Agent acts only for the Cluster its credential was issued for | `carried` | M3 |
| SR-56 | Client authentication at the token endpoint resists abuse | `new` | M3 |
| SR-57 | Exposure of the Agent Gateway listener and its unauthenticated surface | `needs decision` | M3 |
| SR-58 | Credentials for internal service-to-service calls | `needs decision` | M3 |
| SR-59 | The Agent sends credentials only over TLS and never an empty one | `carried` | M3 |
| SR-60 | Agent commands are limited to the operator-validated Project path | `carried` | M3 |
| SR-61 | The command outbox is scoped to one Cluster and dies with its credential | `new` | M3 |
| SR-62 | Mock build and unserved plugin gRPC services | `obsolete` | M3 |
| SR-63 | A submission is authorized by Role before anything else is looked up | `carried` | M4 |
| SR-64 | The target Cluster is resolved only inside the caller's Organization | `carried` | M4 |
| SR-65 | Submission input is validated against itself | `carried` | M4 |
| SR-66 | Error responses carry user-facing messages only | `carried` | M4 |
| SR-67 | An unknown Project name creates a Project in the caller's Organization only | `new` | M4 |
| SR-68 | A Deployment's content never changes after submission | `new` | M4 |
| SR-69 | The CLI stores credentials in owner-only files | `carried (code unchanged)` | M4 (re-verify) |
| SR-70 | A stored CLI token is sent only to the API it was issued for | `carried (code unchanged)` | M4 (re-verify) |
| SR-71 | The CLI accepts secrets from files, not command-line arguments | `carried (code unchanged)` | M4 (re-verify) |
| SR-72 | The CLI executes no project-controlled code and validates what it renders | `carried (code unchanged)` | M4 (re-verify) |
| SR-73 | Log reads require the logs-read permission in the owning Organization | `carried` | M5 |
| SR-74 | Log queries are always scoped to one authorized Deployment and its tenant namespace | `carried` | M5 |
| SR-75 | User-supplied filter text cannot alter the log query | `carried` | M5 |
| SR-76 | Only an authenticated Agent writes logs, and only for its own Cluster | `carried` | M5 |
| SR-77 | The WebSocket origin is validated before any cookie is used | `carried` | M5 |
| SR-78 | Subscriptions are authorized per resource, and unauthenticated clients cannot subscribe | `carried` | M5 |
| SR-79 | Subscription volume is bounded | `carried` | M5 |
| SR-80 | No stream is served for an identifier the caller cannot be shown to own | `carried` | M5 |
| SR-81 | Backend errors from log streams are replaced by generic messages | `carried` | M5 |
| SR-82 | Realtime state never outlives or crosses sessions | `carried` | M5 |
| SR-83 | Updates reach only the subscribers they belong to | `carried` | M5 |
| SR-84 | Malformed frames cannot crash or stall the realtime endpoint | `carried` | M5 |
| SR-85 | Log content masking | `needs decision` | M5 |
| SR-86 | App installation setup requires signed state, the right Role, and a visible installation | `carried` | M6 |
| SR-87 | An installation belongs to at most one Organization | `carried` | M6 |
| SR-88 | Webhooks are authenticated by signature before they are parsed or acted on | `carried` | M6 |
| SR-89 | Webhook events resolve to Projects by repository identity | `carried` | M6 |
| SR-90 | Importing a repository requires deployment permission and stays inside the Organization | `carried` | M6 |
| SR-91 | Importing a repository executes none of its code on the Control Plane | `carried` | M6 |
| SR-92 | Repository access tokens are short-lived, narrowly used, and redacted | `carried` | M6 |
| SR-93 | Credential refresh through webhooks is limited to builds of the matching Project | `carried` | M6 |
| SR-94 | A repository is claimed by at most one Project at a time | `carried` | M6 |
| SR-95 | Previews for pull requests from forks | `needs decision` | M6 |
| SR-96 | Runtime images carry no secret values and default to the hardened profile | `carried` | M7 |
| SR-97 | Inputs to Control Plane deployment workflows are validated before use | `carried` | M7 |
| SR-98 | CI and release workflows that build or ship the Control Plane keep least privilege | `carried` | M7 |
| SR-99 | Required secrets fail fast and placeholders are never used literally | `carried` | M1 |
| SR-100 | The production profile is hardened by default | `carried` | M1 |
| SR-101 | Secrets come from the environment or secret references, never from committed files | `carried` | M1 |
| SR-102 | Secrets are redacted in debug output, logs, and tool output | `carried` | M1 (rule), M4 (CLI), M6 (GitHub types) |
| SR-103 | GitHub App credentials are validated at startup | `carried` | M6 |
| SR-104 | Outbound email is encrypted in transit and carries no credentials | `new` | M2 |

## Sign-in, sessions, and web surface

### SR-01 GitHub is the only browser identity provider

- **Status:** `new`
- **Milestone:** M1
- **Threat:** Every additional credential type (passwords, reset links, emailed verification tokens) is another path to account takeover and another secret store to protect.
- **Requirement:** The Dashboard MUST authenticate people only through GitHub, using the same GitHub App that backs repository integration. The Control Plane MUST NOT store, accept, or verify passwords, and MUST NOT expose password reset or email verification flows.
- **Source:** #915 and #917: browser sign-in is GitHub only, through the same GitHub App used for repository integration.
- **Old tests:** none (new behavior). Negative acceptance test: the route inventory (SR-10) contains no credential-submission endpoint.

### SR-02 A User is identified by the numeric GitHub user ID

- **Status:** `new`
- **Milestone:** M1
- **Threat:** GitHub logins can be renamed and later claimed by someone else, and email addresses can be unverified or reassigned. Matching Users by either lets a different person inherit an existing User's access.
- **Requirement:** The identity key of a User MUST be the numeric GitHub user ID. Login names and email addresses MUST NOT be used to find, merge, or authorize Users. A changed GitHub login MUST update the displayed name of the same User without creating a second User or changing any access.
- **Source:** #915, #917, #918: Invitations resolve to the numeric GitHub user ID and Staff is granted by GitHub user ID.
- **Old tests:** none (new behavior). Replaces the email-based matching tests listed under SR-26.

### SR-03 One GitHub identity maps to exactly one User

- **Status:** `carried`
- **Milestone:** M1
- **Threat:** A second identity attached to one User, or one identity attached to two Users, lets one person act as another and breaks per-User revocation.
- **Requirement:** A GitHub identity MUST map to at most one User and a User MUST have exactly one GitHub identity. The storage layer MUST enforce this itself so that concurrent sign-ins cannot create duplicates. Repeating the same sign-in is idempotent.
- **Source:** 3dd072381 (inside #879).
- **Old tests:** `auth/tests/integration/test_oauth_linking.rs::second_identity_from_the_same_provider_is_rejected`, `auth/tests/integration/test_oauth_storage.rs::concurrent_identities_for_one_user_and_provider_have_one_winner`, `auth/tests/integration/test_oauth_storage.rs::test_unique_provider_user_id_is_enforced`.

### SR-04 Sign-in flow state is single-use, expiring, provider-bound, and browser-bound

- **Status:** `carried`
- **Milestone:** M1
- **Threat:** Login CSRF and authorization-code injection: an attacker starts a sign-in, lures a victim's browser into completing it, and the victim ends up signed in as the attacker (or the attacker's callback is replayed against the victim's session).
- **Requirement:** Each sign-in attempt MUST carry state that (a) is unguessable, (b) is consumed on first use so a replay fails, (c) expires within ten minutes, (d) is valid only for the provider that issued it, and (e) is bound to the browser that started the attempt through a value the browser holds in a short-lived, script-inaccessible, path-scoped cookie that contains no identity data. A callback whose binding value is missing, swapped, or from a different browser MUST be rejected and MUST still consume the state. State MUST be shared across replicas.
- **Source:** #788, #816, #839, fae332194; 6602fee86 (inside #879).
- **Old tests:** `auth/tests/unit/test_oauth_callback_security.rs::callback_rejects_swapped_binding_and_consumes_state`, `auth/tests/unit/test_oauth_callback_security.rs::callback_rejects_provider_swap_and_expired_state`, `auth/tests/unit/test_oauth_state_cookie.rs::oauth_binding_preserves_browser_and_session_boundaries`, `auth/tests/unit/test_oauth_state_cookie.rs::oauth_state_cookie_is_http_only_short_lived_and_contains_only_the_nonce`, `auth/tests/unit/test_oauth_state_cookie.rs::expired_oauth_cookie_clears_the_matching_path`, `auth/tests/integration/test_oauth_storage.rs::contextual_oauth_state_is_shared_and_consumed_once`.

### SR-05 Sign-in never attaches an identity to a session it did not start

- **Status:** `needs decision`
- **Milestone:** M1
- **Threat:** If a callback can attach a GitHub identity to whichever User happens to hold a session in the browser ("ambient" linking), an attacker can bind their identity to a victim's User, or take over a victim's User by luring them through the attacker's callback.
- **Requirement (conditional):** If any flow exists that attaches an additional or replacement GitHub identity to an existing User, it MUST be initiated from an authenticated session, its ownership MUST be kept on the server side (not in the browser), and it MUST fail if the session ends, rotates, or changes User before the callback completes. The target User MUST be active. Such a flow MUST NOT exist as a side effect of ordinary sign-in.
- **Question:** SR-02 and SR-03 make the GitHub identity the User. Is there still any link or unlink flow (for example moving a User to a new GitHub account)? If not, this record becomes `obsolete` and the invariant is covered by SR-03. If yes, the requirement above applies unchanged.
- **Source:** #769 (duplicate #770), 2412efa6b and 7aad3f8df (inside #879).
- **Old tests:** `auth/tests/unit/test_oauth_state_cookie.rs::account_link_ownership_requires_matching_server_context_and_active_session_user`, `auth/tests/integration/test_oauth_linking.rs::test_authenticated_link_attaches_to_current_user`, `auth/tests/integration/test_oauth_linking.rs::test_target_only_link_rejects_provider_owned_by_another_user`, `auth/tests/integration/test_oauth_linking.rs::test_target_only_link_leaves_no_provider_link_for_revoked_membership`.

### SR-06 Provider access tokens are encrypted at rest

- **Status:** `carried`
- **Milestone:** M1
- **Threat:** A database read (backup, replica, injection) discloses long-lived GitHub user tokens that can read the User's repositories and installations.
- **Requirement:** Any GitHub token the Control Plane persists MUST be stored encrypted with a key that is not stored in the database. Sign-in MUST NOT start when the key is absent or malformed. Ordinary reads of an identity record MUST NOT return the token; callers that need it MUST request it explicitly. A wrong key MUST fail decryption rather than return garbage.
- **Source:** 04a7a3174 (inside #693); documented in `docs/tools/dashboard.md`, "GitHub OAuth".
- **Old tests:** `auth/tests/unit/test_oauth_settings.rs::test_github_disabled_when_token_encryption_key_missing`, `auth/tests/unit/test_oauth_settings.rs::test_github_disabled_when_token_encryption_key_invalid`, `auth/services/oauth/token_crypto.rs::test_access_token_encryption_roundtrip`, `auth/services/oauth/token_crypto.rs::test_access_token_decryption_rejects_wrong_key`, `auth/tests/integration/test_oauth_storage.rs::test_store_token_for_user_encrypts_and_loads_explicitly`.

### SR-07 Sessions are revalidated against the current User

- **Status:** `carried`
- **Milestone:** M1
- **Threat:** A session that carries a copy of the User's privileges outlives the User's deactivation, deletion, or demotion, so a removed or demoted User keeps access until the session expires.
- **Requirement:** On every request, the privileges of a session MUST be derived from the current User record, not from data stored in the session. A deactivated or deleted User MUST be treated as anonymous immediately. A change to Staff status MUST take effect on the next request without sign-in.
- **Source:** 2412efa6b (inside #879); 41e73789e (inside #248).
- **Old tests:** `auth/tests/integration/test_validated_session_middleware.rs::active_cookie_session_uses_current_database_privileges`, `auth/tests/integration/test_validated_session_middleware.rs::inactive_cookie_session_user_becomes_anonymous`, `auth/tests/integration/test_validated_session_middleware.rs::deleted_cookie_session_user_becomes_anonymous`.

### SR-08 Session cookies are hardened, bounded in lifetime, and destroyed on sign-out

- **Status:** `carried`
- **Milestone:** M1
- **Threat:** Session theft through script access, plaintext transport, or cross-site sending; session fixation; sessions that never expire.
- **Requirement:** Session cookies MUST be script-inaccessible, `SameSite=Lax` or stricter, and `Secure` in every profile except local development. A new session identifier MUST be issued at sign-in. A session MUST expire after at most 30 minutes of inactivity and at most 24 hours in total. Sign-out MUST destroy the session on the server, not only clear the cookie. Session state MUST be shared across replicas.
- **Source:** #294 (031b0664d, 6a70b3476); configuration in `config/urls.rs` (`create_cookie_session_config`).
- **Old tests:** `auth/services/session.rs::test_session_id_from_cookie_header`, `auth/services/session.rs::test_session_service_factory_resolves_with_overridden_redis_url`, `dashboard/tests/e2e/auth_dashboard_clusters_deployments_github/browser_session.rs::session_is_deleted_when_its_guard_leaves_scope`. Cookie attributes, lifetimes, and server-side destruction on sign-out: `gap`.

### SR-09 Private pages and server endpoints require an authenticated session

- **Status:** `carried`
- **Milestone:** M1
- **Threat:** Unauthenticated access to Dashboard routes or server functions.
- **Requirement:** Every Dashboard route and server function not listed in SR-10 MUST reject anonymous callers, and the Dashboard MUST send anonymous browsers to sign-in instead of rendering private pages.
- **Source:** 9ea714a9f (inside #879).
- **Old tests:** `gap` for the rejection itself; the browser suite `dashboard/tests/e2e/auth_dashboard_clusters_deployments_github/test_dashboard_pages.rs::dashboard_routes_load_without_style_or_overflow_regressions` only exercises authenticated loads.

### SR-10 The unauthenticated surface is enumerated

- **Status:** `carried`
- **Milestone:** M1
- **Threat:** A route added without an authentication decision is public by accident.
- **Requirement:** The set of endpoints reachable without a session or API Key MUST be a short, explicit list, and every other endpoint MUST deny anonymous callers. The old list was: sign-in start and callback, the health endpoint (SR-15), GitHub webhooks (authenticated by signature, SR-88), static assets, and API documentation (SR-11). An acceptance test MUST enumerate the registered routes and fail when one outside the list answers an anonymous request with success.
- **Source:** `config/urls.rs` (session skip list); #408.
- **Old tests:** `gap`.

### SR-11 API documentation exposure in deployed profiles

- **Status:** `needs decision`
- **Milestone:** M1
- **Threat:** Public OpenAPI, Swagger UI, and ReDoc pages disclose the full API surface to anonymous callers and ease reconnaissance.
- **Requirement (if served):** API documentation MUST NOT include secrets or example credentials, and MUST NOT make the endpoints it documents callable without authentication.
- **Question:** The old Control Plane served `/api/openapi.json`, `/api/docs`, and `/api/redoc` to anonymous callers in every profile and tested them in the Bruno suite. Should deployed profiles keep serving them anonymously, restrict them to signed-in Users, or disable them?
- **Source:** `config/urls.rs` (session skip list).
- **Old tests:** `dashboard/tests/bruno/scenarios/01 OpenAPI JSON.bru`, `02 Swagger UI.bru`, `03 ReDoc.bru`, `dashboard/tests/openapi_test.rs`.

### SR-12 Cross-site request protection for cookie-authenticated requests

- **Status:** `carried`
- **Milestone:** M1
- **Threat:** Cross-site request forgery against state-changing endpoints authenticated by the browser's cookie.
- **Requirement:** State-changing requests authenticated by a session cookie MUST be rejected unless the request proves same-origin intent, either through an `Origin` header that matches an explicit allow-list or through a request token. The allow-list MUST be explicit configuration; a wildcard MUST be ignored. Localhost origins MAY be added only in debug profiles. Requests authenticated by an API Key (an `Authorization` header) are not subject to this rule because the browser never attaches that header on its own.
- **Source:** #294 (94f4deb73, a68d282e6, ca956db5b); #451 (5a80858e5, explicit form token).
- **Old tests:** `config/urls.rs::debug_allowed_origins_include_configured_and_active_port`, `config/urls.rs::debug_allowed_origins_fall_back_when_configured_only_wildcard`, `config/urls.rs::production_allowed_origins_do_not_add_localhost_fallbacks`. Rejection of a cross-origin state-changing request: `gap`.

### SR-13 Security response headers

- **Status:** `carried`
- **Milestone:** M1
- **Threat:** Script injection and downgrade attacks through missing browser policy.
- **Requirement:** API responses MUST carry a content security policy that allows nothing (`default-src 'none'`). Page responses MUST carry a policy that restricts scripts to the application's own origin plus WebAssembly evaluation. Strict transport security MUST be sent only when the request is known to have arrived over HTTPS, which is trusted only behind a configured TLS-terminating proxy. The admin site MAY use a separate, documented policy.
- **Source:** `config/middleware/security_headers.rs`.
- **Old tests:** `config/middleware/security_headers.rs::prefixed_admin_paths_are_detected_as_admin_routes` only. Header values: `gap`.

### SR-14 Internal errors do not reach clients

- **Status:** `carried`
- **Milestone:** M1
- **Threat:** Database driver text, constraint names, backend URLs, and stack details help an attacker map the system.
- **Requirement:** Responses to failures the caller cannot act on MUST be generic. Validation and conflict messages that are meant for the caller MAY be specific, but MUST NOT contain driver text, SQL, constraint names, internal hostnames, or upstream error bodies. Details go to server logs only.
- **Source:** #806 (aba7887e7).
- **Old tests:** `deployments/server_urls.rs::test_deployment_error_response_hides_internal_errors`, `auth/server_fn/login.rs::test_internal_error_uses_generic_application_error`, `auth/server_fn/register.rs::registration_keeps_unmapped_database_errors_private`, `auth/server_fn/register.rs::registration_maps_known_unique_fields_without_driver_text`. The `login.rs` and `register.rs` tests are not portable: their subjects (password sign-in and registration) are removed (SR-21, SR-24). Only `deployments/server_urls.rs::test_deployment_error_response_hides_internal_errors` ports directly; the other two serve as models for the rebuilt sign-in error tests.

### SR-15 The health endpoint is cheap and exposes only coarse status

- **Status:** `carried`
- **Milestone:** M1 (database probe), M3 (Agent Gateway probe)
- **Threat:** An unauthenticated endpoint that hits dependencies on every call is a denial-of-service amplifier, and a verbose one leaks topology.
- **Requirement:** `GET /api/healthz/` MAY be anonymous. It MUST return only the stable `{status, db, grpc}` shape with `ok` or `error` values and HTTP 503 on failure. Dependency probes MUST have a short timeout (the old limit was 2 seconds) and their results MUST be cached briefly (the old limit was 1 second) so request rate does not translate into dependency load. A hung dependency MUST NOT hang the endpoint.
- **Source:** #408; #798 (duplicate #820), #851.
- **Old tests:** `health/tests/integration/test_healthz.rs::test_healthz_returns_200_when_all_probes_succeed`, `health/tests/integration/test_healthz.rs::test_healthz_returns_503_when_db_down`, `health/tests/integration/test_healthz.rs::test_healthz_returns_503_when_grpc_unreachable`. Cache and timeout behavior: `gap`.

### SR-16 A Login Link can be used once

- **Status:** `new`
- **Milestone:** M1
- **Threat:** A Login Link that survives its first use turns a leaked URL (browser history, proxy log, chat message) into a standing credential.
- **Requirement:** A Login Link MUST be consumable exactly once. Consumption MUST be atomic so that two concurrent requests with the same link cannot both succeed. A used, expired, revoked, or unknown link MUST be rejected with the same response, and the response MUST NOT reveal which of those conditions applied.
- **Source:** #915 and #917: definition of Login Link; #923: the self-deploy end-to-end test signs in with a Login Link.
- **Old tests:** none (new behavior).

### SR-17 A Login Link has a short lifetime

- **Status:** `new`
- **Milestone:** M1
- **Threat:** An unused Login Link left in a shell history, CI log, or ticket is a valid credential for as long as it lives.
- **Requirement:** A Login Link MUST expire within a bounded period set at issuance, with a hard ceiling that configuration cannot exceed. The proposed ceiling is 15 minutes; M1 may change it with a recorded rationale. Expiry MUST be evaluated on the server's clock at consumption time.
- **Source:** #915 and #917: definition of Login Link.
- **Old tests:** none (new behavior).

### SR-18 Login Links are issued only by someone with host operator access and are stored hashed

- **Status:** `new`
- **Milestone:** M1
- **Threat:** If a Login Link can be requested over the network, it is a password-less backdoor. If it is stored in clear, a database read yields working sign-ins.
- **Requirement:** Login Links MUST be issuable only through the host-level management command (`manage create-login-link`); no HTTP, WebSocket, or gRPC endpoint may create one. The link MUST be generated from a cryptographically secure source with at least 128 bits of entropy, shown once to the issuer, and stored only as a one-way hash. The issuance MUST name the target User, who MUST be an existing, active User; a Login Link MUST NOT create a User. Issuance and consumption MUST be recorded in a log that omits the secret.
- **Source:** #915 and #917: definition of Login Link; replaces the seeded default password (SR-25).
- **Old tests:** none (new behavior).

### SR-19 The sign-up policy is enforced at first sign-in

- **Status:** `new`
- **Milestone:** M1
- **Threat:** Anyone with a GitHub account becomes a User of a self-hosted Control Plane that was meant to be private.
- **Requirement:** The first sign-in of an unknown GitHub identity MUST be evaluated against the configured policy before any User, Organization, session, or stored token is created:
  - `open`: any identity may sign up.
  - `allowlist`: only identities that are listed, or that are members of a listed GitHub organization, may sign up. Membership MUST be verified against GitHub on the server, not taken from the browser, and MUST be evaluated on the numeric IDs of the User and the organization.
  - `invite_only` (the default): only an identity with a pending Invitation (SR-45) may sign up.

  An identity that fails the policy MUST leave no persistent trace beyond an audit log entry, and MUST see a response that does not reveal the policy contents. An unrecognized or missing policy value MUST resolve to `invite_only`. A User that already exists MUST NOT be affected by later tightening of the policy except where the operator deactivates them explicitly.
- **Source:** #915 and #917: the sign-up policy is configurable (`open`, `allowlist`, `invite_only` by default).
- **Old tests:** none (new behavior).

### SR-20 Staff is granted only by host operator access

- **Status:** `new`
- **Milestone:** M1
- **Threat:** Staff can read and change every Organization's data through the admin site. A network-reachable path to Staff is a path to full compromise.
- **Requirement:** Staff status MUST be grantable and revocable only through the host-level management command (`manage grant-staff --github-user-id <id>`), addressed by numeric GitHub user ID. No endpoint, form, Invitation, API Key, or Login Link may grant it. The admin site MUST be reachable only by Staff.
- **Source:** #915 and #917: Staff is granted with `manage grant-staff --github-user-id <id>`.
- **Old tests:** `auth/tests/integration/test_validated_session_middleware.rs::active_cookie_session_uses_current_database_privileges` covers revalidation of Staff status only (SR-07). The grant path: none (new behavior).

### SR-21 Password credential handling

- **Status:** `obsolete`
- **Milestone:** M1
- **Reason:** The Control Plane no longer stores or verifies passwords (SR-01).
- **Threat (historic):** Offline cracking of stored password hashes; username enumeration; weak passwords.
- **Source:** #248, #283, #287, #294, #331; `Argon2` credential verification.
- **Old tests (to be removed with the code):** `auth/tests/integration/test_credential_service.rs::test_verify_credentials_valid_user`, `::test_verify_credentials_wrong_password`, `::test_verify_credentials_nonexistent_user`, `::test_verify_credentials_inactive_user`, `::test_verify_credentials_whitespace_trimmed`, `auth/tests/unit/test_jwt.rs::test_password_hash_and_verify`, `auth/tests/unit/test_auth_property.rs::test_password_hash_verify_roundtrip`, `auth/tests/unit/test_serializer_validation.rs` login and register boundary tests.
- **Carry-over:** The inactive-User rejection is retained as SR-07.

### SR-22 Email verification and email change

- **Status:** `obsolete`
- **Milestone:** M1
- **Reason:** The Control Plane has no editable email address that grants access (SR-01, SR-02). Email is used only to send Invitations and notifications.
- **Threat (historic):** Taking over an account by changing its email to one the attacker controls before verification.
- **Source:** #383 (123650685, 6d9b5804a), #331.
- **Old tests (to be removed with the code):** `auth/tests/unit/test_email_verification.rs::test_osrng_token_is_32_unique_bytes`, `::test_sha256_of_plain_token_is_deterministic`, `::test_expired_token_is_detected`, `::test_consumed_token_is_detected`, `::test_user_id_mismatch_is_detected`, `::test_valid_token_passes_all_checks`, `auth/services/token.rs::test_email_verification_token_roundtrip`.

### SR-23 Password reset tokens

- **Status:** `obsolete`
- **Milestone:** M1
- **Reason:** No passwords exist to reset (SR-01).
- **Threat (historic):** Reset tokens reused across purposes or after a password change.
- **Source:** #331 (c94ab54e9, 5ff0aa117, cb3650874).
- **Old tests (to be removed with the code):** `auth/tests/unit/test_token_service.rs::test_cross_purpose_rejection`, `::test_password_reset_token_with_project_secret`, `auth/services/token.rs::test_password_reset_token_roundtrip`, `::test_wrong_purpose_returns_error`, `::test_tampered_token_returns_error`, `::test_wrong_secret_key_returns_error`, `::test_password_change_invalidates_reset_token`, `::test_malformed_token_returns_error`.

### SR-24 Registration gating and unverified-account state

- **Status:** `obsolete`
- **Milestone:** M1
- **Reason:** Account creation is governed by the sign-up policy (SR-19) and Invitation fulfillment (SR-46); there is no unverified-account state.
- **Threat (historic):** Provisioning an Organization for an account whose email was never verified; leaving a half-registered Organization behind when the verification email could not be sent.
- **Source:** #682 (2d42ab421), #795 (duplicate #817), #818 (3b0e32c30).
- **Old tests (to be removed with the code):** `auth/tests/integration/test_provisioning.rs` registration tests (`::test_provision_personal_organization_happy_path` and related), `auth/services/registration.rs::personal_org_retry_uses_database_error_kind`. The provisioning idempotence and "do not restore a revoked membership" assertions carry over to SR-46.

### SR-25 Seeded default password

- **Status:** `obsolete`
- **Milestone:** M1
- **Reason:** `seed-self-deploy-user` is removed; the self-deploy end-to-end test signs in with a Login Link (SR-16 to SR-18).
- **Threat (historic):** A public, hard-coded password in the repository and runtime image could create or overwrite an active account.
- **Source:** #842 (7baa2ff3c), 0049aaa78.
- **Old tests (to be removed with the code):** `config/management.rs::validate_seed_password_accepts_non_empty_non_default_secret`, `config/management.rs::validate_seed_password_rejects_empty_or_public_fixture_secret`.

### SR-26 Merging Users by verified email

- **Status:** `superseded`
- **Milestone:** M1
- **Replaced by:** SR-02.
- **Threat (historic):** Signing in with a provider identity whose email matched an existing User attached the identity to that User, so an attacker with a provider account holding the victim's (verified or not) email could take the account; refusing on collision avoided a crash and a takeover.
- **Source:** d3a3909da (inside #446), 1b8154299 (inside #446).
- **Old tests (to be removed with the code):** `auth/tests/integration/test_oauth_linking.rs::test_email_verified_match_links_existing_user`, `::test_email_unverified_collision_returns_email_conflict`, `::test_new_user_created_with_no_password`, `::test_username_collision_appends_suffix`, `::test_email_verified_true_but_no_email_creates_new_user`, `::test_username_falls_back_to_sub_when_no_login_or_name`, `auth/tests/unit/test_oauth_linking_validation.rs::test_email_conflict_display_includes_email_and_provider`. The empty-subject rejection (`::test_empty_sub_returns_missing_claim_error`) carries over to SR-02: a sign-in without a numeric ID MUST fail.

## API Keys

### SR-27 API Keys are unguessable, recognizable, and never stored in clear

- **Status:** `carried`
- **Milestone:** M1
- **Threat:** A database read yields working credentials; short or predictable keys are guessable; unrecognizable keys evade secret scanners when committed by mistake.
- **Requirement:** An API Key MUST contain at least 256 bits from a cryptographically secure source, MUST begin with the fixed `rct_` prefix so secret scanners can detect it, and MUST be stored only as a one-way hash. The plaintext MUST be shown to its owner exactly once, at creation. Listings MUST show only a short non-secret prefix, a label, and timestamps. The failure of the entropy source MUST fail issuance.
- **Source:** a64960fb0 (inside #720).
- **Old tests:** `auth/tests/integration/test_api_key_service.rs::test_generate_then_verify_roundtrip`, `auth/tests/integration/test_api_key_service.rs::test_list_api_keys_for_user_returns_keys`.

### SR-28 API Key verification rejects every invalid state

- **Status:** `carried`
- **Milestone:** M1
- **Threat:** A revoked, expired, unknown, or deactivated-User key continues to authenticate.
- **Requirement:** A bearer credential MUST authenticate only when it matches a stored key that is not revoked, not expired, and belongs to an active User. All rejections MUST be indistinguishable to the caller and MUST yield an anonymous request. The submitted value MUST NOT be accepted from a URL query string.
- **Source:** a64960fb0, d95d2017b (inside #720).
- **Old tests:** `auth/tests/integration/test_api_key_service.rs::test_verify_rejects_revoked_token`, `auth/tests/integration/test_api_key_service.rs::test_verify_rejects_expired_token`, `auth/tests/integration/test_api_token_middleware.rs::test_resolve_valid_token_authenticated`, `auth/tests/integration/test_api_token_middleware.rs::test_resolve_invalid_token_anonymous`, `tests/e2e/cli_auth.rs::api_me_accepts_valid_bearer_token`, `tests/e2e/cli_auth.rs::api_me_rejects_missing_token`, `tests/e2e/cli_auth.rs::api_me_rejects_unknown_token`, `tests/e2e/cli_auth.rs::api_me_rejects_revoked_token`. Deactivated-User rejection: `gap`.

### SR-29 Bearer authentication never erases or elevates a session

- **Status:** `carried`
- **Milestone:** M1
- **Threat:** An invalid `Authorization` header wipes a valid session (denial of service), or ordering of the authentication layers lets stale session state overwrite a valid key (wrong principal).
- **Requirement:** A valid API Key MUST establish the request's principal regardless of any session present. An invalid or malformed bearer value MUST leave the existing session state untouched. A request MUST have exactly one principal.
- **Source:** 56d0f4cfd (inside #720).
- **Old tests:** `auth/tests/integration/test_validated_session_middleware.rs::valid_bearer_token_replaces_validated_cookie_session`, `auth/tests/integration/test_api_token_middleware.rs::test_server_fn_accepts_bearer_token`.

### SR-30 Usage bookkeeping cannot revive or delay anything

- **Status:** `carried`
- **Milestone:** M1
- **Threat:** A last-used update that races with revocation reactivates the key, or a synchronous write on every request becomes a load amplifier.
- **Requirement:** Recording that a key was used MUST NOT modify a revoked key and MUST NOT be on the request's critical path.
- **Source:** a64960fb0 (inside #720).
- **Old tests:** `auth/tests/integration/test_api_key_service.rs::test_touch_last_used_skips_revoked_token`.

### SR-31 Users issue and revoke their own API Keys from the Dashboard

- **Status:** `new`
- **Milestone:** M1
- **Threat:** Without a self-service path, operators mint keys by hand and keys are never rotated; with a careless one, a stolen browser tab or a stolen key mints more keys.
- **Requirement:** An authenticated User MUST be able to create, list, and revoke their own API Keys from the Dashboard. Creation MUST require an interactive session (SR-33 decides whether an API Key may also create keys), accept a label and an optional expiry, and display the plaintext once with a clear statement that it cannot be shown again. A User MUST see and revoke only their own keys. Revocation MUST take effect on the next request that presents the key. Creating and revoking a key MUST be recorded with the User, the key's non-secret prefix, and the time.
- **Source:** #917: M1 exit criteria include API Keys through the Dashboard and `manage`.
- **Old tests:** none (new behavior). The old Control Plane issued keys only through `manage`.

### SR-32 Operator commands manage API Keys

- **Status:** `carried`
- **Milestone:** M1
- **Threat:** Break-glass key management must exist without opening a network path to it.
- **Requirement:** `manage create-api-token`, `manage list-api-tokens`, and `manage revoke-api-token` MUST exist, be usable only with host operator access, and observe SR-27: the plaintext is printed once and listings never print it.
- **Source:** caf900f30 (inside #720).
- **Old tests:** `gap` (commands were exercised through the end-to-end suite only).

### SR-33 Authority carried by an API Key

- **Status:** `needs decision`
- **Milestone:** M1
- **Threat:** A key that inherits everything its owner can do widens the blast radius of a leaked CI credential: Staff powers, key creation, and every Organization the owner belongs to.
- **Requirement (floor):** An API Key MUST NOT confer more authority than its owner holds at the time of the request (never a snapshot taken at issuance), and a deactivated owner's keys MUST stop working (SR-28).
- **Question:** (a) Does a key inherit Staff status? The old Control Plane did (`is_staff` or `is_superuser` flowed into the key's principal). (b) May a key create or revoke other keys, or only a session? (c) A User may belong to several Organizations: how does a key select the Organization for a CLI submission (SR-63) when the target is identified only by Cluster and Project name? (d) Do keys need a scope narrower than "everything the owner can do"?
- **Source:** a64960fb0, d95d2017b (inside #720).
- **Old tests:** `gap`.

## Roles, Organization isolation, and tenancy

### SR-34 Every action is explicitly allowed or denied for every Role

- **Status:** `carried`
- **Milestone:** M2
- **Threat:** A new action that silently defaults to "allowed" for a low Role, or a role that silently inherits what it should not.
- **Requirement:** The Role model (owner, admin, developer, viewer) MUST define, for every action, an explicit allow or deny for every Role; adding an action or a Role MUST fail the build or the test suite until the matrix is completed. The starting matrix is the old one:

  | Role | Allowed |
  |------|---------|
  | owner | every action |
  | admin | every action except deleting the Organization (and subject to SR-43 for owner changes) |
  | developer | read the Organization; create, read, update, and delete Clusters and Deployments; read logs |
  | viewer | read the Organization, Clusters, Deployments, and logs |

  Higher Roles inherit every permission of lower Roles. Actions added for Projects are defined in SR-44.
- **Source:** #453 (db0f35c77, 234c849f4).
- **Old tests:** `organizations/tests/unit/test_permissions.rs::owner_can_perform_every_action`, `::admin_matrix`, `::developer_matrix`, `::viewer_matrix`, `::higher_roles_inherit_viewer_permissions`, `::only_owner_can_delete_org`, `organizations/tests/unit/test_role.rs::membership_role_can_respects_hierarchy`.

### SR-35 Mutations authorize by Role, in the Organization that owns the target

- **Status:** `carried`
- **Milestone:** M2 (rule), M3, M4, M6 (enforcement in `clusters`, `deployments`/`projects`, `github`)
- **Threat:** Checking only that the caller is a Member lets a viewer create Clusters or Deployments; when ownership moved from Users to Organizations, several mutation paths lost their Role check.
- **Requirement:** Every endpoint and server function that creates, changes, or deletes Organization-owned data MUST check that the caller's Role in the Organization that owns the target allows that specific action, before reading or changing the target. Membership alone is not sufficient. The check MUST apply identically to every entry point that reaches the same operation (Dashboard, CLI submission, GitHub import, webhooks acting for a User).
- **Source:** #760, #767 (6f1f38055), #741; 60733db73, 70f7abd90 (inside #453).
- **Old tests:** `gap` for the endpoints. Only the matrix was tested (SR-34). Each owning milestone MUST add a viewer-denied and an owner-allowed test per mutating endpoint.

### SR-36 Organization-owned data is isolated

- **Status:** `carried`
- **Milestone:** M2 (rule), M3, M4, M5, M6 (per application)
- **Threat:** Reading or changing another Organization's Clusters, Projects, Deployments, installations, logs, Previews, or realtime updates by guessing identifiers.
- **Requirement:** Every query and mutation on Organization-owned data MUST be constrained to the Organization the caller is acting in, in the query itself, not only after loading. An identifier that belongs to another Organization MUST behave exactly like one that does not exist. The same holds for batch, list, preview, and realtime paths.
- **Source:** #434 (b79e913a3), #464 (1dc741590), #286.
- **Old tests:** `deployments/tests/integration/test_preview_server_fn.rs::deployment_preview_list_scopes_to_current_org_and_reports_row_errors`, `utils/realtime/consumer.rs::authorize_deployment_subscriptions_skips_cross_org_deployment_guess`, `utils/realtime/consumer.rs::authorize_app_log_subscription_rejects_cross_org_deployment_guess`, `utils/realtime/consumer.rs::authorize_app_log_subscription_rejects_user_without_membership`. HTTP and server-function paths for Clusters, Deployments, and installations: `gap`.

### SR-37 Responses do not reveal which Organizations exist

- **Status:** `carried`
- **Milestone:** M2
- **Threat:** Distinguishing "no such Organization" from "not a Member" lets an attacker enumerate Organization slugs.
- **Requirement:** For a caller who is not a Member, an unknown Organization and a known Organization MUST be indistinguishable in status code, body, and timing class.
- **Source:** 50c4f7bf3 (inside #434); `organizations/permissions/guard.rs`.
- **Old tests:** `gap`.

### SR-38 Clients cannot set server-owned fields

- **Status:** `carried`
- **Milestone:** M3 (`clusters`), M4 (`projects`, `deployments`)
- **Threat:** Mass assignment: a request that names `organization_id`, active state, or credential state moves a record into another Organization or reactivates a revoked one.
- **Requirement:** Request bodies for creating and updating records MUST be limited to an explicit list of client-owned fields. A request that names any other field MUST be rejected at decoding, not ignored. Empty update bodies MUST be rejected. Text fields MUST be trimmed and length-checked on the server regardless of client checks.
- **Source:** 6e4175366 (inside #879); fad6b0cc3 (inside #286).
- **Old tests:** `clusters/server_fn.rs::cluster_create_model_form_rejects_server_owned_fields_at_decode`, `clusters/server_fn.rs::cluster_create_model_form_trims_public_values`, `clusters/server_fn.rs::cluster_create_validation_counts_characters_after_trimming`, `clusters/server_fn.rs::cluster_update_rejects_invalid_normalized_values`, `deployments/tests/unit/test_request_validation.rs::test_deployment_request_missing_fields`.

### SR-39 The tenant namespace is derived on the server and slugs are safe

- **Status:** `carried`
- **Milestone:** M2
- **Threat:** A chosen Organization slug that collides with a system namespace, an unsafe DNS label, or a client-supplied namespace places workloads where they can reach other tenants or the platform.
- **Requirement:** The Kubernetes namespace for an Organization's workloads MUST be `tenant-{organization slug}`, computed by the Control Plane from the Organization record and never taken from a client. An Organization slug MUST be a valid DNS-1123 label (lowercase letters, digits, and hyphens; starting with a letter; ending with a letter or digit) and MUST NOT be one of the reserved names (`kube-system`, `kube-public`, `kube-node-lease`, `default`, `reinhardt-cloud-system`, `system`, `admin`, `api`, `dashboard`, or empty). Names derived from GitHub logins MUST be sanitized into that form.
- **Source:** #434; `TenantRef` in `crates/reinhardt-cloud-types`.
- **Old tests:** `organizations/tests/unit/test_slug.rs::validate_slug_enforces_dns_1123_label`, `::is_reserved_slug_blocks_known_names`, `::sanitize_produces_valid_dns_label`, `::sanitize_truncates_to_63_characters`, `::sanitize_empty_falls_back_to_user_prefix`, `::sanitize_leading_digit_falls_back_to_user_prefix`, `apps/validators.rs` DNS label tests, `crates/reinhardt-cloud-types/src/crd/tenant.rs` namespace tests.

### SR-40 Slug length and the tenant namespace limit

- **Status:** `needs decision`
- **Milestone:** M2
- **Threat:** Availability, not confidentiality: a slug the Control Plane accepts may produce a namespace the operator rejects.
- **Requirement (if the limit is lowered):** Slug validation MUST guarantee that `tenant-{slug}` is itself a valid namespace name.
- **Question:** The old slug rule allowed up to 63 characters, but the namespace adds the seven-character `tenant-` prefix, so a slug of 57 to 63 characters passed the Control Plane and failed the operator's tenant validation. Should the Organization slug limit become 56 characters, or should the namespace rule change?
- **Source:** `organizations/roles.rs` (`MAX_SLUG_LEN`), `crates/reinhardt-cloud-types/src/crd/tenant.rs` (`validate`).
- **Old tests:** `organizations/tests/unit/test_slug.rs::sanitize_truncates_to_63_characters` asserts the 63-character behavior.

### SR-41 Submissions cannot target a namespace outside the Organization's tenant namespace

- **Status:** `needs decision`
- **Milestone:** M4
- **Threat:** A Project manifest that names another tenant's namespace, or a platform namespace, applied to a shared Cluster.
- **Requirement (defense in depth):** A submission whose manifest places the Project in a namespace other than the owning Organization's tenant namespace MUST be rejected by the Control Plane before an Agent command is queued.
- **Question:** Today only the operator rejects such manifests (`TenantMismatch`), after the command has been delivered. The old CLI submission path checked that the manifest's namespace matched the request's `namespace` field but never compared it with the tenant namespace. Should the Control Plane also reject mismatches, or is the operator the single enforcement point?
- **Source:** `deployments/services/submission.rs`; operator `validate_tenant_namespace`.
- **Old tests:** `deployments/tests/unit/test_submit_service.rs::test_validate_submission_manifest_rejects_namespace_mismatch` (request versus manifest only). Operator-side tests are outside this document.

### SR-42 An unknown stored Role fails closed

- **Status:** `carried`
- **Milestone:** M2
- **Threat:** Corrupted or hand-edited Role data is interpreted as a permissive Role.
- **Requirement:** The stored Role value MUST be constrained to the four defined Roles by the database, and a value that cannot be parsed at authorization time MUST result in denial and an internal error, never in any grant.
- **Source:** `organizations/permissions/guard.rs`.
- **Old tests:** `organizations/tests/integration/test_membership_constraints.rs::membership_role_check_rejects_invalid_database_value`, `organizations/tests/unit/test_role.rs::membership_role_parse_from_db_string`.

### SR-43 Owner protection and no Role escalation

- **Status:** `new`
- **Milestone:** M2
- **Threat:** An admin demotes or removes an owner, promotes themselves to owner, or invites a new owner; the last owner is removed and nobody can administer the Organization.
- **Requirement:** An admin MUST NOT change, remove, or promote an owner, and MUST NOT grant any Role above admin (by role change or by Invitation). Only an owner may grant or revoke the owner Role. An Organization MUST always retain at least one owner; removing or demoting the last owner MUST be refused. A Member MUST NOT be able to grant a Role higher than their own.
- **Source:** #915 and #918: definition of Member; the old permission table deferred the owner-change rule to the member-management view, which did not exist (`organizations/permissions/table.rs`).
- **Old tests:** none (new behavior).

### SR-44 Project permissions

- **Status:** `new`
- **Milestone:** M4
- **Threat:** Project is now a first-class entity; without its own actions, Project creation and deletion would borrow Deployment permissions or have none.
- **Requirement:** Project create, read, update, and delete MUST be distinct actions in the Role matrix (SR-34). Developers and above may create, read, update, and delete Projects; viewers may read. Submitting a Deployment (SR-63) requires Deployment create permission and, when the submission creates the Project (SR-67), Project create permission as well.
- **Source:** #915 and #920: Project becomes a first-class entity; the old matrix has no Project action.
- **Old tests:** none (new behavior).

### SR-45 An Invitation is bound to a GitHub user ID

- **Status:** `new`
- **Milestone:** M2
- **Threat:** An Invitation addressed to a GitHub login can be captured by whoever later owns that login (renamed, deleted, or transferred), or by anyone who obtains the email it was announced in.
- **Requirement:** Creating an Invitation MUST resolve the given GitHub login to its numeric GitHub user ID through GitHub at creation time, and the Invitation MUST be stored and matched on that ID. A login that does not resolve MUST be refused. The Invitation MUST be fulfilled only when the identity with that numeric ID signs in; no token in an email, no email address, and no login name may substitute for it. The invitation email, when sent, MUST carry no credential and MUST only point at the sign-in page.
- **Source:** #915 and #918: Invitations address a GitHub login, resolved to the numeric GitHub user ID when created.
- **Old tests:** none (new behavior).

### SR-46 Invitation lifecycle

- **Status:** `new`
- **Milestone:** M2
- **Threat:** Stale, reused, or half-applied Invitations grant access that nobody intended; failures after partial work leave orphaned state.
- **Requirement:** An Invitation MUST carry the Role it grants (bounded by SR-43) and an expiry. It MUST be fulfilled at most once, atomically with the creation of the Member. A revoked, expired, or already-fulfilled Invitation MUST grant nothing. Fulfillment MUST NOT restore a Member who was removed after the Invitation was issued. Fulfillment MUST be idempotent under concurrent sign-ins, and a failure to deliver the invitation email MUST NOT leave the Organization in a half-created state.
- **Source:** #915 and #918: definition of Invitation. Carries the intent of the old provisioning tests: `auth/tests/integration/test_provisioning.rs::test_ensure_personal_organization_is_idempotent`, `::test_ensure_personal_organization_does_not_restore_revoked_membership`, `::test_ensure_personal_organization_is_concurrency_safe`; and of #795 (roll back on email failure).
- **Old tests:** the three provisioning tests above are the models for the rebuilt acceptance tests.

### SR-47 Staff authority and Organization data

- **Status:** `needs decision`
- **Milestone:** M2
- **Threat:** Staff reaches every Organization through the admin site; if the product APIs also treat Staff as an implicit owner of every Organization, a compromised Staff session reads every tenant's logs and credentials.
- **Requirement (floor):** Staff status MUST NOT by itself grant access through Dashboard, CLI, or realtime APIs; access to an Organization's data through those APIs follows Membership and Role only.
- **Question:** The rebuild plan (#915) keeps the reinhardt admin registrations for every model, which gives Staff read and write access to all rows. Is that intentional for production, or should admin access to Organization-owned data and credential-bearing models (API Keys, Cluster credentials, stored provider tokens) be read-only or hidden? How are Staff actions audited?
- **Source:** #915: staff-only admin registrations are kept for every model; old `is_staff` handling in #248.
- **Old tests:** `gap`.

## Agent Gateway and Clusters

### SR-48 Cluster client credentials are shown once and stored hashed

- **Status:** `new`
- **Milestone:** M3
- **Threat:** A database read or a screen share yields the credential that lets anyone impersonate a Cluster's Agent and receive its commands.
- **Requirement:** Registering a Cluster MUST create exactly one OAuth client for it. The client secret MUST be generated from a cryptographically secure source with at least 256 bits, shown to the registering Member once, and stored only as a one-way hash. The client secret MUST NOT appear in listings, logs, audit records, or error messages. Registering a Cluster requires the Cluster create permission (SR-34), and the secret is visible only to the Member who performed the registration or rotation.
- **Source:** #915 and #919: Agent authentication moves to OAuth `client_credentials`. The old equivalent returned the token once and stored an Argon2 hash (#409).
- **Old tests:** `clusters/services/token_issuance.rs::test_agent_token_service_issued_token_contains_cluster_id_claim` covered issuance only. Display-once and hash-at-rest: `gap`.

### SR-49 Agent access tokens are short-lived

- **Status:** `new`
- **Milestone:** M3
- **Threat:** A stolen Agent credential used for 30 days (the old lifetime) gives an attacker a month of command access.
- **Requirement:** An Agent MUST obtain access tokens through the OAuth 2.0 `client_credentials` grant. An access token MUST be valid for at most one hour and MUST identify exactly one Cluster. The Agent Gateway MUST reject an expired token, including on an already established stream once the stream next needs authorization (SR-51 states the bound). No access token may be issued without presenting a current client secret.
- **Source:** #915 and #919: Agents fetch one-hour tokens through OAuth `client_credentials`.
- **Old tests:** none (new behavior). The old lifetime was `AGENT_TOKEN_EXPIRY_HOURS = 24 * 30`.

### SR-50 Revocation and rotation take effect through the OAuth store

- **Status:** `new`
- **Milestone:** M3
- **Threat:** A rotated or revoked Cluster credential keeps working. The old Control Plane shipped exactly this defect: rotation stored a new hash but the gRPC verifier was stateless, so rotated tokens stayed valid until expiry (#757).
- **Requirement:** Rotating a Cluster's client secret MUST invalidate the previous secret immediately. Deactivating or deleting a Cluster MUST revoke its client and every outstanding access token. Revocation state MUST be consulted on token issuance and on every Agent Gateway authorization; it MUST NOT rely on expiry alone. A rotation or revocation MUST be recorded with the Member who performed it.
- **Source:** #915 and #919: revocation and secret rotation use the OAuth store; lesson of #757 (d99202832).
- **Old tests:** none for the OAuth store (new behavior). The old stateful check is covered by SR-52.

### SR-51 Revocation latency is bounded

- **Status:** `needs decision`
- **Milestone:** M3
- **Threat:** Between revocation and the next authorization, a revoked Agent keeps its stream and its pending commands.
- **Requirement (floor):** A revoked Cluster MUST be unable to open a new stream or obtain a new token immediately, and MUST lose any established stream within the bound chosen below.
- **Question:** What is the maximum time a revoked Agent may keep an established stream? The rebuild plan (#915) fixes one-hour access tokens but a stream is long-lived. Options: close streams when the Cluster is revoked (immediate); re-check revocation on every command delivery; or accept up to one hour. The old design checked persisted state on every call, which was immediate.
- **Source:** #757; #915 and #919: Agent authentication.
- **Old tests:** `gap` for the new design.

### SR-52 Stateful credential revocation for the old per-Cluster JWT

- **Status:** `superseded`
- **Milestone:** M3
- **Replaced by:** SR-50 and SR-51.
- **Threat (historic):** Rotated or revoked per-Cluster JWTs stayed valid (#757).
- **Source:** #757 (d99202832).
- **Old tests (to be ported to the OAuth store as SR-50 acceptance tests):** `crates/reinhardt-cloud-grpc/src/interceptor.rs::test_agent_interceptor_calls_stateful_token_validator`, `::test_agent_interceptor_rejects_validator_failure`.

### SR-53 Issuance of a 30-day HS256 Agent JWT per Cluster

- **Status:** `superseded`
- **Milestone:** M3
- **Replaced by:** SR-48 and SR-49.
- **Threat (historic):** One signing secret shared by every Cluster and every User session meant a leaked secret forged any Cluster's token; a leaked token lived 30 days.
- **Source:** #409 (de2b5782e, d24f164d6, 5c0b88844).
- **Old tests (to be removed with the code):** `crates/reinhardt-cloud-grpc/src/agent_claims.rs` round-trip tests, `clusters/services/token_issuance.rs::test_agent_token_service_factory_resolves_with_overridden_secret`, `::test_agent_token_service_issued_token_contains_cluster_id_claim`.

### SR-54 Every Agent Gateway call except the standard health check requires an Agent credential

- **Status:** `carried`
- **Milestone:** M3
- **Threat:** Unauthenticated network clients invoking Agent RPCs can open streams, report fake state, or push commands. Two defects shipped before this was enforced: services registered without authentication (#810) and an authentication layer matching a stale service path so it never ran for the real service (#887).
- **Requirement:** Every Agent Gateway RPC other than the standard health check MUST reject a call that has no credential, a malformed credential, an expired or revoked credential, or a credential of the wrong kind (a User session or User credential is not an Agent credential, and an Agent credential is not a User credential). The check MUST be bound to the service paths that the generated server actually exposes. Acceptance tests MUST call every RPC through a real listener, so a mismatched path fails the test.
- **Source:** #810 (duplicate #830), #887, #736.
- **Old tests:** `crates/reinhardt-cloud-grpc/src/interceptor.rs::test_agent_interceptor_validates_agent_token`, `::test_agent_interceptor_rejects_user_token`, `::test_agent_interceptor_rejects_invalid_token`, `::test_agent_interceptor_call_missing_auth_on_agent_path`, `::test_agent_interceptor_accepts_valid_agent_call`, `::test_interceptor_call_missing_auth_header`, `::test_interceptor_call_malformed_bearer`, `::test_interceptor_call_empty_bearer`, `::test_validate_expired_token`. These insert the method identifier into the request by hand; no old test proved the identifier is present when the server runs. A real-listener suite is the replacement.

### SR-55 An Agent acts only for the Cluster its credential was issued for

- **Status:** `carried`
- **Milestone:** M3
- **Threat:** An Agent for Cluster A registers as Cluster B, receives B's commands, or reports B's state and logs.
- **Requirement:** The Cluster an Agent represents MUST be derived solely from its verified credential, never from any field in its messages. A stream opened without a verified Cluster identity MUST be refused. Commands MUST be routed to a Cluster's Agent only. State reports, health, and log pushes MUST be attributed to the credential's Cluster and MUST NOT be accepted for another.
- **Source:** 62a9f3e02 and 5c0b88844 (inside #409).
- **Old tests:** `crates/reinhardt-cloud-grpc/src/registry.rs::test_register_with_cluster_binds_agent_to_cluster`, `::test_send_command_to_cluster_routes_to_right_agent`, `::test_send_command_to_unknown_cluster_fails`. Refusal of an unauthenticated stream and attribution of reports and log pushes: `gap`.

### SR-56 Client authentication at the token endpoint resists abuse

- **Status:** `new`
- **Milestone:** M3
- **Threat:** The token endpoint is reachable over the same port as the Dashboard and accepts a secret; guessing or credential stuffing against it, and distinguishing "unknown client" from "wrong secret", reveal which Clusters exist.
- **Requirement:** Failed client authentication MUST return one uniform error that does not reveal whether the client exists, MUST be rate-limited per client and per source, and MUST be recorded. The endpoint MUST accept the client secret only in the request body or an `Authorization` header, never in a URL, and MUST be reachable only over TLS in deployed profiles.
- **Source:** #915 and #919: Agents reach the Control Plane through an HTTP token endpoint (port 8000) in addition to gRPC.
- **Old tests:** none (new behavior).

### SR-57 Exposure of the Agent Gateway listener and its unauthenticated surface

- **Status:** `needs decision`
- **Milestone:** M3
- **Threat:** A listener bound to all interfaces serves anything registered on it to the network; reflection hands attackers the schema.
- **Requirement (floor):** Every service on the listener other than the Agent Gateway's own MUST be unreachable from the network or authenticated per SR-54 and SR-73. The only RPCs answered anonymously MUST be the standard health check, and exposure of the listener beyond the loopback interface MUST be an explicit configuration choice.
- **Question:** (a) The old default bind was `0.0.0.0:50051`, flipped to loopback in #811 and flipped back in c8a61a778 in the same pull request so Agents could reach it; local and CI profiles override it to loopback. Should the rebuilt default be loopback, with deployed profiles setting the exposure explicitly? (b) The old public path list also let anonymous callers use gRPC server reflection. Should reflection be disabled outside development? (c) Who terminates TLS for the gRPC listener (SR-59 requires TLS for authenticated Agents)?
- **Source:** #811 (8b09a1690, c8a61a778), #855, #887; `crates/reinhardt-cloud-grpc/src/interceptor.rs` (`PUBLIC_PATHS`).
- **Old tests:** `crates/reinhardt-cloud-grpc/src/config.rs::test_default_config`, `crates/reinhardt-cloud-grpc/src/config.rs::test_bind_address_brackets_ipv6_hosts`.

### SR-58 Credentials for internal service-to-service calls

- **Status:** `needs decision`
- **Milestone:** M3
- **Threat:** A shared signing secret that mints User-level credentials is a single point of compromise; if the Control Plane retains one without a clear owner it will be left weak or unrotated.
- **Requirement (floor):** If the Control Plane calls its own services over a network interface, those calls MUST present a credential that the callee verifies, MUST NOT be accepted from outside the process boundary without that credential, and the credential MUST be marked sensitive in logs and traces.
- **Question:** `REINHARDT_CLOUD_JWT_SECRET` is in the preserved operator-injected environment contract. Its consumers were (a) the per-Cluster Agent JWT (superseded by OAuth, SR-53) and (b) the short-lived User JWT the Dashboard minted to call its own log service (`utils/realtime/consumer.rs`). After the rebuild, what does the secret secure, if anything? If nothing, should it be removed from the preserved contract?
- **Source:** #810, 0769fd411; `utils/grpc.rs::dashboard_grpc_auth_interceptor_marks_authorization_sensitive`.
- **Old tests:** `utils/grpc.rs::dashboard_grpc_auth_interceptor_marks_authorization_sensitive`.

### SR-59 The Agent sends credentials only over TLS and never an empty one

- **Status:** `carried`
- **Milestone:** M3
- **Threat:** A long-lived Agent credential sent over plaintext HTTP/2 or HTTP can be read by anyone on the path; an Agent that connects without a credential is accepted by a misconfigured server.
- **Requirement:** The Agent MUST refuse to send a client secret, access token, or `Authorization` header to a Control Plane URL that is not TLS-protected, including the token endpoint. The Agent MUST refuse to start with an empty credential. TLS verification MUST use the platform trust roots and MUST NOT be disabled by default.
- **Source:** #748, #749.
- **Old tests:** `crates/reinhardt-cloud-agent/src/main.rs::test_validate_control_plane_transport_rejects_plaintext_with_token`, `::test_validate_control_plane_transport_allows_https_with_token`, `::test_validate_control_plane_transport_allows_plaintext_without_token`, `::build_auth_header_rejects_empty_token`, `::build_auth_header_formats_bearer_token`.

### SR-60 Agent commands are limited to the operator-validated Project path

- **Status:** `carried`
- **Milestone:** M3
- **Threat:** A command stream that can create arbitrary workloads in the Cluster turns a compromised Control Plane credential or stream into cluster-wide code execution (#749).
- **Requirement:** The Control Plane MUST send an Agent only commands that apply or remove `Project` resources, plus the credential Secrets a Project's source build needs (SR-93). The Agent MUST reject any other command kind, including the legacy direct deployment command, and the operator remains the only component that turns a `Project` into workloads.
- **Source:** #749.
- **Old tests:** `crates/reinhardt-cloud-agent/src/main.rs::handle_command_rejects_legacy_deploy_command`.

### SR-61 The command outbox is scoped to one Cluster and dies with its credential

- **Status:** `new`
- **Milestone:** M3
- **Threat:** The rebuild stores commands in a database outbox so they survive disconnects. Without scoping, a queued command is delivered to the wrong Agent, or a revoked Agent drains commands queued before revocation.
- **Requirement:** Each outbox entry MUST belong to exactly one Cluster and be delivered only on a stream authenticated for that Cluster (SR-55). Revoking or deleting a Cluster MUST stop delivery of its pending entries. Entries MUST NOT contain credentials beyond what the Project apply needs, and delivered Secrets MUST be removed from the outbox once acknowledged. An entry MUST NOT be redelivered after acknowledgment. Any replica may enqueue, but only the replica holding the Agent's stream may deliver.
- **Source:** #915 and #919: Agent commands are written to a database outbox.
- **Old tests:** none (new behavior).

### SR-62 Mock build and unserved plugin gRPC services

- **Status:** `obsolete`
- **Milestone:** M3
- **Reason:** The mock `BuildService` and the unserved `PluginService` descriptors are dropped (dropped by the rebuild decision in #915). No listener serves them, so nothing remains to authenticate.
- **Threat (historic):** Build control APIs reachable from the network without credentials (#810).
- **Source:** #810.
- **Old tests (to be removed with the code):** `tests/integration/tests/build_grpc_integration.rs` and the build-service unit tests; the authentication assertions for non-build services carry over to SR-54.

## CLI submission

The CLI HTTP contract (`GET /api/auth/me/`, `POST /api/deployments/cli/`, bearer `rct_` API Keys, error body `{ "error": "..." }`) is preserved unchanged.

### SR-63 A submission is authorized by Role before anything else is looked up

- **Status:** `carried`
- **Milestone:** M4
- **Threat:** A viewer, or a member of another Organization, submits a Deployment. The CLI path once checked only that the caller was authenticated (#767).
- **Requirement:** `POST /api/deployments/cli/` MUST require an authenticated principal and MUST check that the principal's Role in the Organization that owns the target Cluster allows creating a Deployment, before loading the Cluster, validating the manifest against it, or queueing any Agent command. Malformed JSON and failed validation MAY be answered before authorization only if the response is identical for every caller.
- **Source:** #767 (6f1f38055), #729 (a00e9dfcb), #760.
- **Old tests:** `gap` for the endpoint. The error-body tests are listed under SR-66.

### SR-64 The target Cluster is resolved only inside the caller's Organization

- **Status:** `carried`
- **Milestone:** M4
- **Threat:** Naming another Organization's Cluster, which may share a name with the caller's, applies a Project to it.
- **Requirement:** The Cluster named in a submission MUST be looked up within the Organization determined by SR-33 and SR-63. A Cluster in any other Organization MUST produce the same "not found" response as a nonexistent one. An inactive Cluster, or one without an API URL, MUST be refused.
- **Source:** #767.
- **Old tests:** `gap`. The inactive-Cluster conflict is covered by `deployments/server_urls.rs::test_deployment_error_response_exposes_user_facing_errors`.

### SR-65 Submission input is validated against itself

- **Status:** `carried`
- **Milestone:** M4
- **Threat:** A request whose fields disagree with its manifest applies something other than what was authorized, reviewed, or recorded: a different Project name, a different namespace, or a different image.
- **Requirement:** The submission MUST be refused with a client error unless: the Project name is 1 to 63 characters; the image is 1 to 512 characters; the manifest parses as a valid Project; the manifest's name equals the request's `project_name`; the manifest's namespace equals the request's `namespace` (an absent manifest namespace counts as `default`); and the manifest's image equals the request's `image`. Inputs MUST be trimmed before comparison, and an empty manifest MUST be refused.
- **Source:** #729 (a00e9dfcb).
- **Old tests:** `deployments/tests/unit/test_submit_service.rs::test_validate_submission_manifest_accepts_matching_project`, `::test_validate_submission_manifest_accepts_trimmed_project_name`, `::test_validate_submission_manifest_rejects_name_mismatch`, `::test_validate_submission_manifest_rejects_namespace_mismatch`, `::test_validate_submission_manifest_rejects_image_mismatch`, `::test_validate_submission_manifest_rejects_missing_yaml`, `deployments/tests/unit/test_cli_serializers.rs::test_cli_deployment_request_validation_boundaries`, `deployments/tests/unit/test_request_validation.rs::test_deployment_project_name_boundary`, `::test_deployment_cluster_id_boundary`, `::test_deployment_image_boundary`, `deployments/services/manifest.rs::test_validate_project_manifest_reports_invalid_spec`.

### SR-66 Error responses carry user-facing messages only

- **Status:** `carried`
- **Milestone:** M4
- **Threat:** Agent connectivity errors and internal failures disclose cluster topology or implementation detail to anyone who can submit.
- **Requirement:** Every error response from the CLI endpoints MUST have the body `{ "error": "<message>" }`. Validation (400) and conflict (409) messages MAY be specific. An unavailable Agent (503) and any internal failure (500) MUST use a fixed generic message; the detail goes to server logs.
- **Source:** #729; SR-14.
- **Old tests:** `deployments/server_urls.rs::test_deployment_error_response_exposes_user_facing_errors`, `::test_deployment_error_response_hides_internal_errors`, `crates/reinhardt-cloud-cli/src/client.rs::test_submit_deploy_maps_503_error_body`, `::test_submit_deploy_maps_409_error_body`.

### SR-67 An unknown Project name creates a Project in the caller's Organization only

- **Status:** `new`
- **Milestone:** M4
- **Threat:** The preserved contract creates a Project with a manual source when `project_name` is unknown. If the name lookup is not scoped to the Organization, a submission can attach to, overwrite, or reveal the existence of another Organization's Project of the same name.
- **Requirement:** A submission MUST resolve `project_name` within the Organization that owns the target Cluster. If no such Project exists there, the Control Plane MUST create one with a manual source in that Organization, which requires the Project create permission (SR-44). If the Project exists in another Organization, the submission MUST NOT touch it and MUST NOT reveal that it exists.
- **Source:** #915 and #920: the CLI contract is preserved, including creating a Project with a manual source when `project_name` is unknown.
- **Old tests:** none (new behavior).

### SR-68 A Deployment's content never changes after submission

- **Status:** `new`
- **Milestone:** M4
- **Threat:** If the manifest or image of a recorded Deployment can be edited, the audit trail no longer shows what was applied and a rollback restores something else.
- **Requirement:** The manifest, image, target Cluster, submitter, and submission time of a Deployment MUST be immutable after submission; only its status may advance. Staff edits through the admin site MUST NOT bypass this (SR-47).
- **Source:** #915 and #920: definition of Deployment.
- **Old tests:** none (new behavior). The old API allowed updating a Deployment's configuration (`DeploymentUpdate`).

### SR-69 The CLI stores credentials in owner-only files

- **Status:** `carried (code unchanged)`
- **Milestone:** M4 (re-verify)
- **Threat:** Another local User reads the stored API Key.
- **Requirement:** The CLI MUST create its credentials directory with owner-only permissions and its credentials file with owner-only permissions applied atomically at creation, and MUST repair a pre-existing permissive file when it next writes the credentials.
- **Source:** #732, #750.
- **Old tests:** `crates/reinhardt-cloud-cli/src/config.rs::test_save_token_sets_restrictive_permissions`, `::test_save_token_repairs_existing_permissive_file`, `::test_save_token_creates_parent_directory`, `::test_save_and_load_token_roundtrip`.

### SR-70 A stored CLI token is sent only to the API it was issued for

- **Status:** `carried (code unchanged)`
- **Milestone:** M4 (re-verify)
- **Threat:** A saved token is replayed to a different (possibly attacker-controlled) API URL chosen through a flag or config file.
- **Requirement:** A saved token MUST record the API URL that issued it and MUST be used only when the selected API URL is the same (ignoring a trailing slash). A token saved without an API URL MUST NOT be used. An explicit flag or environment variable takes precedence over saved credentials.
- **Source:** #754.
- **Old tests:** `crates/reinhardt-cloud-cli/src/config.rs::test_credentials_without_api_url_are_not_scoped`, `::test_credentials_scope_ignores_trailing_slash`, `::test_resolve_token_ignores_unscoped_file_credentials`, `::test_resolve_token_ignores_mismatched_file_credentials`, `::test_resolve_token_uses_matching_file_credentials`, `::test_resolve_token_priority_flag_over_env_over_file`.

### SR-71 The CLI accepts secrets from files, not command-line arguments

- **Status:** `carried (code unchanged)`
- **Milestone:** M4 (re-verify)
- **Threat:** Secrets passed as arguments appear in process listings and shell history.
- **Requirement:** CLI commands that take Git or registry credentials MUST read them from files or the environment and MUST reject blank values. Credential status output MUST NOT print secret values.
- **Source:** #808; 2dead60a9 and 687130752 (inside #295).
- **Old tests:** `crates/reinhardt-cloud-cli/src/commands/credentials.rs::read_secret_file_rejects_blank_file`, `::read_secret_file_preserves_non_newline_whitespace`, `tests/e2e/credentials.rs`.

### SR-72 The CLI executes no project-controlled code and validates what it renders

- **Status:** `carried (code unchanged)`
- **Milestone:** M4 (re-verify)
- **Threat:** Running `deploy` inside an untrusted checkout executes the project's own binaries; metadata read from the project (package name, toolchain, `wasm-bindgen` version) containing shell metacharacters or newlines injects instructions into the generated Dockerfile.
- **Requirement:** The CLI MUST NOT discover or execute a binary from the project tree during introspection. Values read from project files and rendered into build instructions (package names, toolchain channels, dependency versions) MUST be validated against a strict character set before use, and the lookup of project metadata MUST NOT walk above the workspace root.
- **Source:** #771, #789 (duplicate #812), #790 (duplicate #813).
- **Old tests:** `crates/reinhardt-cloud-cli/src/commands/deploy.rs::test_manage_introspect_command_does_not_discover_project_manage_binary`, `crates/reinhardt-cloud-cli/src/feature_detector.rs::test_detect_project_rejects_shell_metacharacters_in_package_name`, `crates/reinhardt-cloud-cli/src/dockerfile_generator/cargo_lock_reader.rs::rejects_wasm_bindgen_version_with_shell_metacharacters`, `crates/reinhardt-cloud-cli/src/dockerfile_generator/rust_toolchain_reader.rs::r10_rejects_channel_with_newline_injection`, `::r11_ignores_toolchain_above_workspace_root`.

## Logs and realtime

### SR-73 Log reads require the logs-read permission in the owning Organization

- **Status:** `carried`
- **Milestone:** M5
- **Threat:** Application logs contain request data and secrets by accident; unauthenticated or cross-tenant reads, and unauthenticated log injection, were all possible before the log service was authenticated (#736).
- **Requirement:** Every log read, historical or live, MUST require an authenticated principal whose Role in the Organization owning the Deployment allows reading logs. This applies to every transport that can carry logs, including any internal one.
- **Source:** #736, #810, 0769fd411.
- **Old tests:** `crates/reinhardt-cloud-grpc/src/interceptor.rs::test_interceptor_requires_auth_for_log_service_path`, `::test_interceptor_accepts_user_token_for_log_service_path`, `::test_log_service_interceptor_accepts_agent_token_for_push_logs`, `::test_log_service_interceptor_rejects_user_token_for_push_logs`, `::test_log_service_interceptor_accepts_user_token_for_list_logs`, `::test_log_service_interceptor_rejects_agent_token_for_list_logs`. The Role check on the Dashboard path: `gap`.

### SR-74 Log queries are always scoped to one authorized Deployment and its tenant namespace

- **Status:** `carried`
- **Milestone:** M5
- **Threat:** A query built from a user-controlled project name alone matches same-named Projects in other Organizations, other tenants' streams, or platform logs, including through substring matching; a missing filter selects everything.
- **Requirement:** The Control Plane MUST resolve the Deployment through the caller's Organization, then query logs with the Deployment's identity and the Organization's tenant namespace (SR-39), matched exactly, never by substring. Every layer that can serve a log read, including the backend adapter, MUST reject a query that has neither a project nor a Deployment scope.
- **Source:** #731, #735, #755, #736.
- **Old tests:** `crates/reinhardt-cloud-grpc/src/services/log.rs::test_scoped_read_filter_rejects_empty_scope`, `::test_scoped_read_filter_accepts_source_or_deployment_id`, `crates/reinhardt-cloud-core/src/services/log/buffer.rs::test_filter_by_source_requires_exact_match`, `crates/reinhardt-cloud-telemetry/src/log_service/loki/query.rs::namespace_label_only_when_set`, `::deployment_id_label_only_when_set`, `deployments/tests/integration/test_deployment_logs.rs::list_logs_filters_by_source_project_name`, `deployments/tests/unit/test_query_v2.rs::deployment_log_queries_use_exact_keys_with_a_shared_family`. The tenant-namespace value actually sent by the Dashboard: `gap`.

### SR-75 User-supplied filter text cannot alter the log query

- **Status:** `carried`
- **Milestone:** M5
- **Threat:** Quotes, backslashes, newlines, or regular-expression syntax in a label value or search term break out of the log query and widen it.
- **Requirement:** Every user-supplied value placed into a log backend query MUST be escaped for the backend's query language in the context where it is used (label value, line filter, regular expression).
- **Source:** 64618edc4 (inside #718).
- **Old tests:** `crates/reinhardt-cloud-telemetry/src/log_service/loki/query.rs::label_values_escape_quotes_backslashes_and_newlines`, `::search_string_escapes_regex_and_logql_literal_characters`, `::search_is_appended_as_escaped_regex_line_filter`.

### SR-76 Only an authenticated Agent writes logs, and only for its own Cluster

- **Status:** `carried`
- **Milestone:** M5
- **Threat:** Anyone who can reach the log write path injects forged entries into another tenant's logs.
- **Requirement:** Log ingestion MUST accept only an Agent credential (SR-54) and MUST attribute entries to that credential's Cluster (SR-55). A User credential MUST NOT be accepted for ingestion, and an Agent credential MUST NOT be accepted for reads.
- **Source:** #736.
- **Old tests:** the interceptor tests listed under SR-73. Attribution to the credential's Cluster: `gap`.

### SR-77 The WebSocket origin is validated before any cookie is used

- **Status:** `carried`
- **Milestone:** M5
- **Threat:** Cross-site WebSocket hijacking: a malicious page opens a socket to the Control Plane and the browser attaches the victim's session cookie.
- **Requirement:** The realtime handshake MUST require an `Origin` header that matches an explicit allow-list, normalized for case and trailing slash, and MUST do so before reading or accepting the session cookie. A missing `Origin` MUST be refused, and a connection without a valid session MUST be refused at the handshake instead of being accepted as anonymous. The allow-list MUST be explicit configuration; a wildcard MUST be ignored.
- **Source:** #835 (duplicate #843); d8e3c5150 (inside #294).
- **Old tests:** `utils/realtime/consumer.rs::test_validate_websocket_origin_rejects_missing_origin`, `::test_validate_websocket_origin_allows_configured_origin`, `::test_normalize_origin_trims_trailing_slash_and_case`, `::test_extract_cookie_value_single`, `::test_extract_cookie_value_multiple`, `::test_extract_cookie_value_missing`. Rejection of a non-listed origin: `gap`.

### SR-78 Subscriptions are authorized per resource, and unauthenticated clients cannot subscribe

- **Status:** `carried`
- **Milestone:** M5
- **Threat:** A client subscribes to updates or logs for Deployments it does not own by sending guessed identifiers.
- **Requirement:** A subscription request MUST be refused for an unauthenticated connection. Each identifier in a request MUST be authorized against the caller's Organization membership and Role; identifiers the caller may not read MUST be skipped or refused without revealing that they exist. Authorization MUST be set-based (one query per batch), not one query per identifier, so that large batches cannot be used as a load amplifier.
- **Source:** #841, 1006fe4f1 (inside #888).
- **Old tests:** `utils/realtime/consumer.rs::test_parse_subscribe_without_auth_rejected`, `::test_parse_subscribe_with_auth_returns_subscribe_action`, `::authorize_deployment_subscriptions_allows_current_org_deployments`, `::authorize_deployment_subscriptions_skips_cross_org_deployment_guess`, `::authorize_app_log_subscription_allows_deployment_in_current_org`, `::authorize_app_log_subscription_rejects_user_without_membership`, `::authorize_app_log_subscription_rejects_cross_org_deployment_guess`, `::test_parse_subscribe_app_logs_with_auth`, `::test_parse_subscribe_app_logs_without_auth_rejected`, `::test_parse_unsubscribe_logs_without_auth_rejected`.

### SR-79 Subscription volume is bounded

- **Status:** `carried`
- **Milestone:** M5
- **Threat:** One client subscribes to an unbounded number of resources, exhausting memory and database capacity.
- **Requirement:** The number of resources in one subscription request, and the total number of subscriptions per User, MUST be bounded (the old limit was 100). An oversized request MUST be refused, and a client that needs more MUST partition its requests.
- **Source:** #888 (4bff8a3b2, 1006fe4f1).
- **Old tests:** `utils/realtime/consumer.rs::test_parse_subscribe_rejects_oversized_batch`, `utils/realtime/broadcaster.rs::test_subscription_limit_enforced`, `::test_try_subscribe_idempotent`.

### SR-80 No stream is served for an identifier the caller cannot be shown to own

- **Status:** `carried`
- **Milestone:** M5
- **Threat:** Build log streams were addressed by identifiers that were not tied to an Organization, so any authenticated User could read any build's logs.
- **Requirement:** A log stream MUST NOT be served unless ownership of the addressed resource by the caller's Organization can be established. Where it cannot, the request MUST be refused with a message that explains the requirement and nothing else.
- **Source:** #809 (duplicate #829).
- **Old tests:** `utils/realtime/consumer.rs::test_parse_subscribe_build_logs_with_auth_rejected_until_ownership_verified`, `::test_parse_subscribe_build_logs_without_auth_rejected`, `::build_log_rejection_explains_deployment_scoped_requirement`.

### SR-81 Backend errors from log streams are replaced by generic messages

- **Status:** `carried`
- **Milestone:** M5
- **Threat:** Upstream error text reveals backend addresses, query text, or other tenants' identifiers.
- **Requirement:** A failure of a log or update stream MUST reach the client as a fixed generic message. The detail goes to server logs.
- **Source:** #806 (duplicate #827).
- **Old tests:** `utils/realtime/consumer.rs::test_build_log_stream_unavailable_uses_generic_message`, `::test_app_log_stream_unavailable_uses_generic_message`.

### SR-82 Realtime state never outlives or crosses sessions

- **Status:** `carried`
- **Milestone:** M5
- **Threat:** A new User on the same browser inherits the previous User's subscriptions, or a closed session keeps receiving updates.
- **Requirement:** Connections and subscriptions MUST be released when the connection ends. After sign-out and sign-in as another User, the browser MUST NOT carry subscriptions, reconnect timers, or cached data from the previous session. Subscriptions MUST be keyed by the authenticated User of the connection, not by anything the client supplies.
- **Source:** 2ff79ec03 (inside #879).
- **Old tests:** `utils/realtime/broadcaster.rs::test_cleanup_user_removes_connections_and_subscriptions`, `::test_connection_full_lifecycle`, `::test_remove_one_connection_keeps_subscriptions`, `utils/realtime/consumer.rs::test_on_disconnect_cleans_up_broadcaster`, `dashboard/tests/wasm/test_notification_lifecycle.rs::a_new_session_does_not_inherit_notification_subscriptions`, `::ending_a_session_cancels_the_pending_notification_reconnect`. Server-side closing of sockets whose session ended: `gap`.

### SR-83 Updates reach only the subscribers they belong to

- **Status:** `carried`
- **Milestone:** M5
- **Threat:** A Deployment status update is delivered to Users who did not subscribe to it.
- **Requirement:** A Deployment status broadcast MUST be delivered only to connections that hold an authorized subscription for that Deployment. A system notification MAY go to every connection.
- **Source:** #841.
- **Old tests:** `utils/realtime/broadcaster.rs::test_broadcast_deployment_status_reaches_subscribers_only`, `::test_broadcast_system_notification_reaches_all`, `::test_broadcast_to_empty_room_noop`.

### SR-84 Malformed frames cannot crash or stall the realtime endpoint

- **Status:** `carried`
- **Milestone:** M5
- **Threat:** A single crafted frame terminates the connection handler or the process.
- **Requirement:** Frames that are not valid messages MUST produce a protocol error and MUST NOT panic, stall, or leak state. Message handling MUST be robust against arbitrary input.
- **Source:** `gap`: no hardening change was identified. The property was only asserted by tests added with the broad coverage work in #257.
- **Old tests:** `shared/ws_messages.rs::test_ws_client_message_fuzz_no_panic`, `::test_invalid_json_returns_error`.

### SR-85 Log content masking

- **Status:** `needs decision`
- **Milestone:** M5
- **Threat:** Applications that log request bodies or credentials expose them to every Member with logs-read, and to anyone who obtains one such session.
- **Requirement (floor):** The Dashboard MUST state that log content is shown as received.
- **Question:** The old Control Plane streamed logs without server-side masking and documented the limitation (`docs/tools/dashboard.md`). Should the rebuild mask well-known secret patterns, restrict logs-read to roles above viewer, or keep the documented limitation?
- **Source:** `docs/tools/dashboard.md`, "Security note".
- **Old tests:** none.

## GitHub integration

### SR-86 App installation setup requires signed state, the right Role, and a visible installation

- **Status:** `carried`
- **Milestone:** M6
- **Threat:** A low-privilege Member, or an attacker who tricks an administrator into following a link, binds a GitHub App installation to an Organization (CSRF on the setup callback), or binds an installation the signed-in User cannot see on GitHub.
- **Requirement:** The setup callback MUST require a state value that is signed with a server secret, expires within ten minutes, and is bound to the initiating User and Organization; it MUST be verified in constant time. The caller MUST hold the Organization-update permission. The installation named in the callback MUST be one that GitHub reports as visible to that User through the User's own token. A mismatch in any of these MUST fail without side effects.
- **Source:** #740 (487afcfcf).
- **Old tests:** `github/tests/unit.rs::test_github_setup_state_round_trips_for_same_user_and_org`, `::test_github_setup_state_rejects_different_organization`, `::test_github_setup_state_rejects_tampered_signature`, `::test_github_install_url_with_state_appends_query_parameter`, `::test_github_app_client_lists_user_installations_with_headers`. The Role and visibility checks of the callback: `gap`.

### SR-87 An installation belongs to at most one Organization

- **Status:** `carried`
- **Milestone:** M6
- **Threat:** A second Organization claims an installation already bound elsewhere and thereby gains access to its repositories.
- **Requirement:** Binding an installation that is already bound to another Organization MUST be refused with a conflict. Re-binding to the same Organization MUST refresh its metadata and set it active.
- **Source:** 8b472eb63 (inside #691); `github/server_urls.rs` (`upsert_verified_installation`).
- **Old tests:** `gap`.

### SR-88 Webhooks are authenticated by signature before they are parsed or acted on

- **Status:** `carried`
- **Milestone:** M6
- **Threat:** Anyone who can reach the webhook URL forges push and pull-request events that trigger builds, credential refreshes, or Preview changes.
- **Requirement:** The webhook endpoint is anonymous at the session level and MUST verify the request's HMAC-SHA-256 signature against the raw body with the configured secret, in constant time, before parsing the payload or touching any state. A missing or malformed signature header MUST be refused. A failed verification MUST cause no side effect and MUST be logged without the payload.
- **Source:** 18b9995da (inside #295); #686 (3cacf4b35).
- **Old tests:** `utils/vcs/signature.rs::test_verify_github_signature_valid`, `::test_verify_github_signature_invalid`, `::test_verify_github_signature_wrong_prefix`, `::test_verify_github_signature_invalid_hex`, `tests/e2e/webhook.rs::github_webhook_signature_rejects_invalid_signature`.

### SR-89 Webhook events resolve to Projects by repository identity

- **Status:** `carried`
- **Milestone:** M6
- **Threat:** Matching on repository name or URL lets an event for a same-named repository trigger another Organization's Project.
- **Requirement:** A webhook event MUST be matched to Projects by the numeric GitHub repository ID. A payload without a repository ID MUST be rejected. Events for branches and pull requests a Project does not track MUST be ignored.
- **Source:** #686 (3cacf4b35).
- **Old tests:** `github/tests/unit.rs::test_github_webhook_dispatch_reads_repository_id_and_action`, `::test_github_webhook_dispatch_rejects_payload_without_repository_id`, `::test_webhook_action_applies_production_push_only_for_tracked_branch`, `::test_webhook_action_applies_preview_lifecycle_annotations`.

### SR-90 Importing a repository requires deployment permission and stays inside the Organization

- **Status:** `carried`
- **Milestone:** M6
- **Threat:** A viewer imports a repository and thereby triggers builds and workloads (#741). An import that names an installation, repository, or Cluster from another Organization uses its credentials or resources.
- **Requirement:** Importing a repository MUST require the same permissions as submitting a Deployment (SR-63) and Project create permission (SR-44). The installation, the repository, and the target Cluster MUST all belong to the caller's Organization (SR-36). Project names derived from a repository MUST be valid (SR-65), and registry values MUST be validated.
- **Source:** #741 (1303eb234).
- **Old tests:** `github/tests/unit.rs::test_import_spec_derives_valid_default_project_name`, `::test_validate_project_name_rejects_invalid_names`, `::test_validate_registry_rejects_whitespace`. The Role and Organization checks: `gap`.

### SR-91 Importing a repository executes none of its code on the Control Plane

- **Status:** `carried`
- **Milestone:** M6
- **Threat:** Running a repository's own `manage` binary or `cargo run` during import executes attacker-controlled code with the Control Plane's privileges and secrets.
- **Requirement:** Deriving Project metadata from a repository MUST NOT build or execute anything from that repository. Metadata MUST come from static reads of files and from Control Plane-controlled tooling, and every value read MUST be validated before use.
- **Source:** #742 (51335af1c).
- **Old tests:** `github/tests/unit.rs::test_github_deploy_pipeline_uses_safe_metadata_without_repository_execution`.

### SR-92 Repository access tokens are short-lived, narrowly used, and redacted

- **Status:** `carried`
- **Milestone:** M6
- **Threat:** An installation token leaked through a log line, a URL in an error message, or a debug dump gives read access to private repositories.
- **Requirement:** Repository access MUST use installation tokens minted per use and scoped to the repository. A token embedded in a clone URL MUST be percent-encoded, and the type that holds it MUST redact the token in its textual representation. Private keys and webhook secrets MUST be redacted in debug output (SR-102).
- **Source:** 457efaf93 (inside #686); #768.
- **Old tests:** `github/tests/unit.rs::test_installation_clone_url_percent_encodes_token_and_redacts_display`, `::test_github_app_settings_debug_redacts_secrets`.

### SR-93 Credential refresh through webhooks is limited to builds of the matching Project

- **Status:** `carried`
- **Milestone:** M6
- **Threat:** Webhook-driven credential refresh pushes fresh repository credentials into Clusters for events that do not need them, or for the wrong Project, widening the lifetime and reach of the token.
- **Requirement:** Credentials MUST be refreshed only for webhook actions that start a source build or a Preview build, only for the Project the event resolved to, and only into that Project's Cluster. Other actions (closing a pull request, ignored branches) MUST NOT refresh them.
- **Source:** #768, #833.
- **Old tests:** `github/server_urls.rs::test_webhook_credential_refresh_includes_source_build_actions`, `::test_webhook_credential_refresh_excludes_non_build_actions`, `github/tests/unit.rs::test_credentials_secret_for_private_repository_only`.

### SR-94 A repository is claimed by at most one Project at a time

- **Status:** `carried`
- **Milestone:** M6
- **Threat:** Two concurrent imports of the same repository, or a stale half-finished one, produce duplicate Projects or leave a repository permanently locked.
- **Requirement:** Importing a repository MUST claim it atomically, the claim MUST expire (the old lease was bounded and clock-safe), a live import MUST be able to renew its lease without being displaced by stale-claim recovery, and deleting the resulting Project MUST release the claim.
- **Source:** dcafc46fc (inside #879).
- **Old tests:** `github/tests/integration/test_import_claim_lease.rs::heartbeat_winning_the_cas_prevents_stale_recovery`, `::stale_recovery_winning_the_cas_prevents_heartbeat_renewal`, `github/tests/unit.rs::import_claim_expiry_is_bounded_and_clock_safe`, `deployments/tests/integration/test_preview_server_fn.rs::deleting_github_deployment_releases_repository_import_claim`.

### SR-95 Previews for pull requests from forks

- **Status:** `needs decision`
- **Milestone:** M6
- **Threat:** A pull request from a fork triggers a Preview build with the Project's repository credentials, registry access, and Cluster resources, which lets the fork's author run code in the tenant's namespace.
- **Requirement (floor):** A Preview MUST NOT be built from a pull request whose head repository the Project's Organization does not control unless a Member with Deployment create permission has explicitly approved that pull request.
- **Question:** The old webhook handler mapped any pull request event for a tracked repository to Preview create, update, and delete (`test_webhook_action_applies_preview_lifecycle_annotations`) and contained no fork check on the Control Plane side. Should previews for fork pull requests be refused, approved per pull request, or allowed?
- **Source:** `github/server_urls.rs` webhook handling; `tests/e2e/webhook.rs`.
- **Old tests:** `gap`.

## Supply chain and build

### SR-96 Runtime images carry no secret values and default to the hardened profile

- **Status:** `carried`
- **Milestone:** M7
- **Threat:** An image that ships a local-development profile and a settings loader that defaults to it starts in an unsafe mode when an environment variable is forgotten; settings files copied into an image may carry credentials.
- **Requirement:** The Control Plane image MUST select the hardened (production) profile by default. Settings files bundled in an image MUST contain only placeholders that resolve from the environment at startup, never credential values; local, CI, and test profiles MUST NOT ship in the runtime image. Images generated by the CLI for other Projects MUST NOT copy the Project's settings directory.
- **Source:** #834 (51e2bb1f5), #763 (350430feb).
- **Old tests:** `crates/reinhardt-cloud-cli/src/dockerfile_generator/stages.rs::runtime_stage_defaults_to_production_settings`, `::runtime_stage_omits_settings_when_dir_present`, `::runtime_stage_omits_settings_when_dir_absent`, `::runtime_stage_copies_manage_binary`, `crates/reinhardt-cloud-cli/src/dockerfile_generator.rs::snapshot_pages_with_settings`.
- **Note for M0 reviewers:** the rebuild plan (#915) preserves `settings/` and `REINHARDT_CLOUD_CONFIG_DIR=/app/settings` in the Control Plane image layout, while the generic generator deliberately omits both for other Projects. The Control Plane image bundles its own placeholder-only settings; the generator behavior stays as it is.

### SR-97 Inputs to Control Plane deployment workflows are validated before use

- **Status:** `carried`
- **Milestone:** M7
- **Threat:** A release or manual version string reaches a shell, a `sed` script, or an environment file and injects commands or extra variables into a privileged job that holds Cluster credentials.
- **Requirement:** Any workflow that deploys the Control Plane MUST validate externally supplied version and tag values against a strict pattern before they reach a command, a file, or an environment export, and MUST write outputs without allowing newline injection.
- **Source:** #737, #756.
- **Old tests:** none; the workflow was unhooked by the M0 teardown and is re-added in M7.

### SR-98 CI and release workflows that build or ship the Control Plane keep least privilege

- **Status:** `carried`
- **Milestone:** M7
- **Threat:** Workflow secrets or write tokens reach untrusted code; third-party actions or installers change under the pipeline; untrusted pull requests run on self-hosted machines.
- **Requirement:** Workflows restored or rewritten for the Control Plane MUST: pin third-party actions and tool installers to immutable references; grant jobs the minimum token permissions and expose the registry token only to publishing steps; not upload artifacts that contain secret-backed values; and not select self-hosted runners for pull requests from forks or untrusted branches.
- **Source:** #744, #747, #753, #804 (duplicate #825), #805 (duplicate #826), #831, #840.
- **Old tests:** none (workflow properties; verify by review and by a workflow lint in M7).

## Configuration and secrets

### SR-99 Required secrets fail fast and placeholders are never used literally

- **Status:** `carried`
- **Milestone:** M1
- **Threat:** A deployed profile starts with an empty, default, or literal-placeholder secret. The old Control Plane once consumed the text of an unexpanded placeholder as its JWT signing key, which let anyone who read the committed file forge Agent tokens (#743).
- **Requirement:** In deployed profiles, startup MUST fail with a clear message when any required secret (session or application secret key, database password, cache URL with credentials, token-encryption key, GitHub App private key and webhook secret, SMTP credentials) is absent, empty, or still holds an unexpanded placeholder. Values for numeric and boolean settings MUST NOT be able to carry a secret placeholder through to use. Committed settings files MUST be self-contained for deployed profiles without any local profile present.
- **Source:** #743; 837d1e261 (inside #495); 1d72b7cd7 (inside #667).
- **Old tests:** `config/settings.rs::test_production_profile_fails_fast_when_required_env_vars_missing`, `::test_production_profile_round_trips_with_required_env_vars`, `::test_get_jwt_secret_interpolates_deployed_profile_placeholder`, `::test_get_jwt_secret_reads_from_runtime_env`, `::test_deployed_profile_toml_self_contained_without_base_toml`, `::dotenv_profile_file_feeds_toml_interpolation`, `::process_env_overrides_dotenv_profile_file`.

### SR-100 The production profile is hardened by default

- **Status:** `carried`
- **Milestone:** M1
- **Threat:** Debug mode, plaintext cookies, missing transport security, wildcard hosts, or permissive origins in production.
- **Requirement:** The production profile MUST disable debug mode, mark session and CSRF cookies secure, redirect to HTTPS and send HSTS (SR-13), list allowed hosts explicitly, define no wildcard or localhost origins (SR-12), and require the Control Plane's own secrets from the environment (SR-99). Development-only conveniences (localhost origins, debug error pages) MUST be unreachable outside debug profiles.
- **Source:** `dashboard/settings/production.toml`.
- **Old tests:** `config/urls.rs::production_allowed_origins_do_not_add_localhost_fallbacks`, `config/settings.rs::test_production_profile_round_trips_with_required_env_vars`.

### SR-101 Secrets come from the environment or secret references, never from committed files

- **Status:** `carried`
- **Milestone:** M1
- **Threat:** Credentials committed to the repository, to example files, or to images.
- **Requirement:** Database passwords, SMTP credentials, and other secrets MUST be supplied through environment variables or secret references and MUST NOT appear as literals in any committed settings file, devcontainer file, or example. The environment-injection contract with the operator (`REINHARDT_ENV`, `REINHARDT_CORE__SECRET_KEY`, `REINHARDT_CLOUD_REDIS_URL`, `REINHARDT_CLOUD_REDIS_PASSWORD`, `REINHARDT_DATABASE_{HOST,PORT,NAME,USER,PASSWORD}`, and the secrets listed in SR-99) MUST keep working; `REINHARDT_CLOUD_JWT_SECRET` is subject to the decision in SR-58.
- **Source:** aa570eddd (inside #523); c2647000d (inside #560).
- **Old tests:** `config/settings.rs::test_email_runtime_env_overrides_profile_toml`, `::test_get_redis_url_prefers_runtime_env_over_profile_toml`, `::get_redis_url_prefers_dotenv_env_over_profile_toml`.

### SR-102 Secrets are redacted in debug output, logs, and tool output

- **Status:** `carried`
- **Milestone:** M1 (rule), M4 (CLI), M6 (GitHub types)
- **Threat:** A debug print, a panic message, a log line, or the CLI's status output discloses a key, token, or password.
- **Requirement:** Types that hold secrets MUST NOT print them through debug or display formatting. Authorization metadata sent over internal calls MUST be marked sensitive so transport logging omits it. Credential-handling commands MUST NOT echo secret values. Request and error logging MUST NOT include `Authorization` headers, cookies, client secrets, Login Links, or webhook payload bodies.
- **Source:** 687130752 and 2dead60a9 (inside #295); #768.
- **Old tests:** `github/tests/unit.rs::test_github_app_settings_debug_redacts_secrets`, `::test_installation_clone_url_percent_encodes_token_and_redacts_display`, `utils/grpc.rs::dashboard_grpc_auth_interceptor_marks_authorization_sensitive`, `auth/tests/unit/test_oauth_providers_view.rs::test_providers_response_does_not_contain_secret_keywords`, `::test_provider_entry_serializes_only_public_provider_fields`. Request-log redaction: `gap`.

### SR-103 GitHub App credentials are validated at startup

- **Status:** `carried`
- **Milestone:** M6
- **Threat:** A blank or mangled private key (for example one with escaped newlines that decode to nothing) lets the application start and fail later in unpredictable, partially-authenticated ways.
- **Requirement:** The GitHub App identifier, private key, and webhook secret MUST be required together. A private key that is missing, blank, or blank after unescaping MUST fail startup of the integration.
- **Source:** #686.
- **Old tests:** `github/tests/unit.rs::test_github_app_settings_loads_required_env`, `::test_github_app_settings_rejects_missing_private_key`, `::test_github_app_settings_rejects_escaped_blank_private_key`.

### SR-104 Outbound email is encrypted in transit and carries no credentials

- **Status:** `new`
- **Milestone:** M2
- **Threat:** SMTP credentials or Invitation emails sent over plaintext; emails that act as credentials.
- **Requirement:** Outbound email MUST use implicit TLS to the configured provider (Cloudflare Email Service in deployed profiles, a local mail catcher in development), with credentials supplied through SR-101. All sending MUST go through one backend interface so the provider can be swapped without changing callers. Email content MUST NOT contain a token that grants access (SR-45), and failure to send MUST NOT be reported to the recipient or caller with provider detail (SR-14).
- **Source:** #915 and #918: outbound email uses the built-in SMTP backend with a swappable provider.
- **Old tests:** `auth/services/email.rs::test_email_service_uses_configured_from_email`, `::test_email_service_factory_honors_console_backend` cover configuration only. Transport security: none (new behavior).

## Appendix A: Sources mined

- `git log origin/main` (2,332 commits) filtered by subject and body for security, hardening, vulnerability, RBAC, CSRF, origin, redaction, authentication, token, secret, session, tenant, and revocation terms, restricted to `dashboard/`, `crates/reinhardt-cloud-grpc/`, `crates/reinhardt-cloud-cli/`, `crates/reinhardt-cloud-agent/`, `crates/reinhardt-cloud-types/`, and `tests/`.
- Merged pull requests whose head branch starts with `codex/` (107 pull requests), all of which were read for motivation, changed files, and added tests. The `vulnerability in:title` search returns only the disclosure-policy pull request (#86).
- Merged pull requests that carried security fixes inside larger changes: #248, #257, #283, #286, #287, #294, #295, #331, #383, #408, #409, #434, #446, #451, #453, #464, #495, #523, #560, #667, #686, #691, #693, #718, #720, #729, #879.
- Existing tests under `dashboard/src/**`, `dashboard/tests/**`, `crates/reinhardt-cloud-grpc/**`, `crates/reinhardt-cloud-core/**`, `crates/reinhardt-cloud-telemetry/**`, `crates/reinhardt-cloud-cli/**`, `crates/reinhardt-cloud-agent/**`, `tests/e2e/**`, and `tests/integration/**`.
- `docs/tools/dashboard.md`, `docs/tools/agent.md`, `docs/tools/cli.md`, `SECURITY.md`, and `dashboard/settings/production.toml`.

Pull requests read and judged not security-relevant: #847 (lint fix).

Duplicate pull requests for one fix: #769/#770; #788/#816/#839; #806/#827; #809/#829; #810/#830; #795/#817; #798/#820; #835/#843; #789/#812; #790/#813; #814/#791; #815/#792; #821/#800; #823/#802; #825/#804; #826/#805; #853/#857/#866; #856/#858.

## Appendix B: Fixes outside the Control Plane

These security fixes landed on `main` but are enforced by the operator, Terraform generation, the Helm charts, or repository CI. They are not owned by a Control Plane milestone. They are listed so that every `security`-type change in the sweep is accounted for; SR-98 covers the CI constraints that apply to workflows the rebuild restores.

| Area | Pull requests |
|------|---------------|
| Operator workload and tenant isolation | #733, #734, #738, #739, #745, #746, #751, #752, #758, #759, #772, #773, #785, #786, #793, #799, #800 (#821), #801, #824, #828, #832, #836, #837, #852, #885, #890 |
| Plugin and types validation | #819 |
| Terraform generation | #761, #762, #791 (#814), #853 (#857, #866), #854 |
| Helm chart and documented RBAC | #792 (#815), #802 (#823), #856 (#858) |
| Repository CI and policy | #744, #747, #753, #804 (#825), #805 (#826), #831, #838, #840 |

