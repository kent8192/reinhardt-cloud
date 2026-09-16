# Task Prompts

Use the smallest template that describes the outcome. Replace bracketed fields
with concrete task data; omit fields already established in the conversation.
Repository rules are inherited, so do not paste the full instructions into every
task. File lists are entry points unless explicitly marked as hard boundaries.

State the desired delivery stage. The examples default to local changes. For a
publication task, name the repository, branch/PR, and authorized actions such as
commit, normal push, Draft PR creation, replies, or thread resolution. PR creation
does not implicitly authorize readiness, merge, or deployment. Reuse permissions
already supplied instead of asking again. Delegation remains opt-in.

## 1. Implement a behavior or fix a bug

```text
In [repository/worktree], implement [observable outcome].
Current behavior: [trigger and actual result].
Expected behavior: [expected result and relevant failure cases].
Entry points: [files/symbols or issue].
Constraints: [compatibility requirements and hard boundaries, if any].
Acceptance: [observable checks for completion].
Delivery: verified local changes; report changed paths, checks, and remaining limits.
```

Infer routine details from the code and proceed through verification. Add a
regression test when it distinguishes the defect from the intended behavior.

## 2. Repair a CI failure

```text
Repair the failing checks for [PR URL or run ID] on its authoritative worktree.
Inspect the failed leaf-job logs and tested head before changing code. Distinguish
code regressions from dependency or infrastructure failures. Make the smallest
complete repair and run the failing check or its local equivalent.
Delivery: [local fix / commit and normal push to the named source branch].
Report root cause, validation, and the actual remote head/CI state if pushed.
```

A queued run is not a failure. Broaden local checks when the failure or changed
shared behavior warrants them; preserve an existing PR's branch and review scope.

## 3. Address review feedback

```text
For [PR URL], address all actionable [named reviewers / bot] feedback in scope.
Collect every page of threads, comments, and review bodies. Evaluate each finding
against the current code, repair valid issues, and verify the resulting behavior.
Authorization: [local fixes only / commit, normal push, English replies, and
resolution of addressed threads].
If publication is authorized, reply with evidence before resolving and re-audit
the full inventory after each push. Report addressed and remaining findings,
head SHA, and actual CI state. Do not merge.
```

For a read-only review, request findings with file locations, impact, and evidence,
and explicitly say that edits and GitHub posting are outside scope.

## 4. Migrate a framework dependency

```text
Migrate [target crate/application] from its locked Reinhardt version to [target
version/ref]. Inspect the current dependency, target changelog, and affected APIs.
Use the installed framework skills only where their examples match that revision.
Preserve [required behavior and compatibility constraints]. Apply the migration
through affected consumers and verify native/WASM surfaces where shared code
crosses that boundary.
Delivery: verified local changes and an accounting of breaking changes handled,
checks, and upstream blockers. Do not upgrade unrelated dependencies.
```

## 5. Change configuration or operator behavior

```text
Implement [behavior] for [configuration generation / operator resource].
Inputs: [Cargo features, settings, introspection, or CRD fields].
Expected outputs: [affected Dockerfile, Cloud TOML, Project manifest, Terraform,
resource state, or status conditions].
Acceptance: [valid/invalid cases, ownership, repeated reconciliation, and cleanup].
Validate with existing fixtures or disposable infrastructure.
Delivery: verified local changes; live deployment is outside scope.
```

## 6. Diagnose dashboard behavior

```text
Fix [user-visible dashboard behavior] in [page/route/component].
Reproduction: [actions, actual result, and expected result].
Read dashboard instructions and confirm the locked framework APIs. Reproduce the
behavior in the appropriate browser/native environment, then repair and verify
the same interaction, including [relevant error or boundary case].
Use the current agent and the repository's existing browser test setup.
Delivery: local changes with browser evidence and native/WASM checks as applicable.
```

A compilation result does not replace interaction evidence. If infrastructure
prevents reproduction, report the exact missing prerequisite and what was verified.

## 7. Revise instructions or documentation

```text
Review [files/scope] for conflicting rules, duplication, stale references, and
unclear completion criteria. Make the smallest coherent revision while preserving
authorization, security, and project-specific behavior. Keep mirrored instruction
files synchronized. Validate changed links, examples, structured prompts, and the
effective instruction input where available.
Delivery: reviewed local changes and validation results. User configuration and
installed plugin caches are outside scope.
```

Instruction shortening is not itself evidence of better task performance. Keep
hard requirements discoverable through explicit, task-specific references.

## 8. Resume established work

```text
Continue [objective/task ID] in [authoritative worktree and branch].
Accepted decisions and constraints: [list].
Authorization already granted: [actions and scope].
Completed work: [files/commits and checks with head or timestamp].
Outstanding work: [acceptance criteria still unmet].
Next step: [concrete action].
Verify the current checkout and resume from this state. Incorporate later
corrections without restarting completed work or reopening approved decisions.
```
