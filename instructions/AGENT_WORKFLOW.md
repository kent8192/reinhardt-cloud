# Agent Workflow

## Establish the task

Record the intended behavior, acceptance criteria, base branch, authoritative
worktree, and requested delivery stage. Inspect the existing diff before edits.
Use the current files and live PR metadata rather than assuming an older task
summary still describes the checkout. Resume an approved design instead of
reopening settled choices.

An action request authorizes investigation, implementation, and relevant local
verification. Publication follows the repository's Git policy. Continue to the
requested outcome; a plan or partial repair is not completion. A status question
or mid-task correction updates the active task without discarding completed work.

When a routine detail is missing, choose a reversible approach supported by the
code and explain the assumption if it affects the result. Ask for decisions that
materially change the contract or exceed authorization. Before asking to publish,
prepare the complete diff, validation results, and proposed external content.
If a rule blocks an action, cite its exact file and clause, explain the unresolved
condition, and continue independent authorized work.

## Select skills and evidence

Use the session's skill catalog to locate the named or relevant skill. Read its
entry point once, then only the references needed for the current branch of work.
Do not load every Rust library skill for a documentation task or copy installed
plugin files into the repository.

| Situation | Useful skill, when installed |
|-----------|-----------------------------|
| Worktree creation or reuse | `worktree` |
| A failed PR/run | `ci-triage` |
| Automated review feedback | `resolve-bot-reviews`, then the actual reviewer's workflow |
| A specified issue or approved plan task | `issue-driven-worktree` or `plan-driven-task` |
| Framework APIs, Pages, migrations, tests | The matching `reinhardt-agents-plugin` skill |
| Agent instructions or prompts | `writing-for-agents`; `openai-docs` for OpenAI model guidance |

Check framework skill versions against `Cargo.toml`, `Cargo.lock`, and the actual
upstream source for that revision. A newer plugin's API example does not establish
that an older dependency provides it. Keep the dashboard an ordinary framework
consumer; distinguish framework defects from Cloud-specific behavior.

Prefer local source, existing tests, lockfiles, and official version-matched docs.
Use available documentation tools when they answer a real gap. A missing optional
MCP server or advisory tool does not block equivalent investigation with available
tools. Never infer configuration, credentials, or a tool's existence from a skill.

User instructions take precedence over skill guidelines, within platform rules.
A skill cannot authorize commits, GitHub messages, deployment, delegation, or
configuration changes. Reuse authorization already present in the task or project
policy instead of adding another approval round. If a skill requires a pause,
identify and quote the exact relevant instruction; distinguish that requirement
from an optional workflow suggestion.

## Execute and recover

Keep investigation, implementation, review, and verification in the current
agent unless delegation was explicitly requested. If requested, assign bounded
ownership and acceptance criteria, keep shared-file mutations sequential, and
integrate results before final verification. Read-only searches can run together;
inspect every result before dependent edits.

Fix the cause supported by evidence and keep unrelated changes outside the diff.
On failure, inspect the error before retrying. Repair the relevant cause or report
the actual prerequisite; do not weaken checks, reset a source tree, or repeatedly
retry an unchanged failure. For upstream workarounds, follow
[UPSTREAM_ISSUE_REPORTING.md](UPSTREAM_ISSUE_REPORTING.md) before implementation.

For CI, inspect failed leaf-job logs and the tested commit. Separate code failures
from dependency resolution, service availability, and infrastructure failures.
Known rustdoc, docs.rs, or Windows patterns are investigation hints, not diagnoses.

## Verification

Select checks by changed behavior and failure risk. Required CI checks remain
required; scoped local validation does not waive branch protection or merge gates.
Use `Makefile.toml`, `dashboard/Makefile.toml`, `.cargo/nextest.toml`, and affected
workflow files for the real commands. Run Cargo from the verified worktree or with
an absolute `--manifest-path`; do not accidentally test the main checkout.

| Change surface | Local evidence |
|----------------|----------------|
| Prose, instruction, or prompt only | Diff review, changed links/anchors, mirror equivalence, prompt contracts, and TOML/YAML parsing when applicable. Rust workspace builds are unnecessary unless Rust examples or build inputs changed. |
| Rust behavior in one component | A focused regression test where useful, affected crate/feature checks, and relevant format/lint tasks. Reuse adequate existing tests. |
| Shared dependencies, public API, build configuration | Affected consumers and feature combinations; broaden to workspace check/build/test gates when the impact crosses the workspace. |
| Dashboard shared client/server code | Native checks and the affected WASM/browser tests. Successful native compilation alone does not prove browser behavior. |
| Configuration to deployment generation | Representative Cargo/settings/introspection inputs and assertions for affected Dockerfile, Cloud TOML, Project CRD, and Terraform output contracts. |
| Operator, RBAC, finalizers, or migrations | Ownership/isolation, invalid input, repeat reconciliation, and cleanup or rollback cases in disposable infrastructure. No live-cluster deployment is implied. |
| Rustdoc or executable examples | Affected doc tests and `cargo doc --no-deps` with the relevant package/features before publishing a doc-related fix. |

Use cargo-nextest for native tests and the existing browser runner for WASM.
`cargo make test` includes native tests and dashboard browser E2E; it is not a
cheap substitute for identifying the affected target. Broad Rust validation uses
workspace check/build, `cargo make test`, `cargo make fmt-check`, and
`cargo make clippy-check` when warranted by the table or explicitly required.

Use Docker for TestContainers and RAII fixtures for cleanup. Avoid concurrent
Cargo jobs contending for the same build output; isolate build directories when
needed. For batch mutations, inspect a dry-run or explicit target preview first.

Tests must exercise behavior, failure conditions, or acceptance criteria. Do not
add tests that merely restate prose or implementation structure. After the relevant
checks pass, repeat only those invalidated by edits or an unresolved concern.
Report skipped or blocked checks and their reason instead of implying they passed.

## Delivery and resumption

Review the final diff, including unrelated-file and secret checks. For PRs use the
actual base's three-dot diff. Stage only owned changes when committing. Local work
can be complete with a retained, uncommitted diff; publication follows
[COMMIT_GUIDELINE.md](COMMIT_GUIDELINE.md) and [PR_GUIDELINE.md](PR_GUIDELINE.md).

After an authorized push, verify local HEAD, upstream, the remote branch, and PR
`headRefOid` agree. Read remote checks for that head. Queued or running checks are
not passed checks. Review work also requires a fresh, fully paginated inventory
after each push; follow [GITHUB_INTERACTION.md](GITHUB_INTERACTION.md).

The final report states the outcome, worktree/branch, changed paths, checks and
limits, and any commit/remote/PR results actually obtained. Do not infer deployed
behavior from a build, PR readiness from CI, or merge state from a push. Retain
worktrees containing uncommitted or otherwise undelivered work.

A resumption handoff records the original objective, accepted corrections,
authorization, exact checkout/head, changed files, completed evidence, outstanding
work, and the next concrete action. Do not persist credentials or temporary logs
in project documentation.

## Evaluating instruction changes

For GPT-6 Astra, this workflow applies the official guidance on explicit scope,
continued execution, instruction conflicts, delegation, and proportional
verification: [OpenAI model guidance](https://developers.openai.com/api/docs/guides/latest-model#prompting-best-practices).
It does not require a model setting change or claim a measured performance gain.

Review prompt behavior against representative tasks: a prose edit, an isolated
Rust defect, a dashboard browser failure, a CI repair, and an authorized review
closeout. Check that required evidence remains, ordinary work proceeds without
extra approval, and external writes or delegation do not exceed scope. If the
installed Codex supports `codex debug prompt-input`, inspect its rendered input
without inference to verify instruction loading; this does not measure task
quality. Measure comparable completed tasks before claiming token or time savings.
