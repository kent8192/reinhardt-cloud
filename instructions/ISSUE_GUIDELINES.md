# Issue Guidelines

## Purpose

This file defines the issue policy for the Reinhardt Cloud project. These rules ensure clear issue tracking, proper labeling, and consistent issue management.

---

## Language Requirements

### LR-1 (MUST): English-Only Content

**ALL issue titles, descriptions, and comments MUST be written in English.**

- Issue titles MUST be in English
- Issue descriptions MUST be in English
- All comments within issues MUST be in English
- Code examples and error messages may use their original language

**Rationale:** English ensures accessibility for all contributors and maintainers worldwide.

---

## Issue Creation Policy

### IC-1 (MUST): Use GitHub CLI with Task Authorization

Use `gh issue create` for authorized issue creation. Creation, closure, deletion,
and comments require the corresponding task authorization; reuse it when already
provided. Otherwise prepare a concrete title, body, and labels before asking.
Follow the installed template and include at least one type label. Read-only
search and local investigation can proceed without publication approval.

### IC-2 (MUST): Search Before Creating

**ALWAYS** search existing issues before creating a new one:
1. Search open and closed issues
2. Check if the issue has already been reported
3. Review related issues for context

**Example:**
```bash
gh issue list --search "reconciler panic"
gh issue list --state closed --search "deployment"
```

### IC-3 (MUST): Use Existing Issue Templates

Inspect [.github/ISSUE_TEMPLATE/](../.github/ISSUE_TEMPLATE/) before preparing an
issue; available templates and their fields can change. Current templates are:

| Issue type | Template | Type label |
|------------|----------|------------|
| Bug | [Bug report](../.github/ISSUE_TEMPLATE/1-bug_report.yml) | `bug` |
| Feature or API proposal | [Feature request](../.github/ISSUE_TEMPLATE/2-feature_request.yml) | `enhancement` |
| CI/CD | [CI/CD issue](../.github/ISSUE_TEMPLATE/3-ci_cd.yml) | `ci-cd` |

For a category without a dedicated template, use the nearest applicable existing
structure and choose the correct type label from
[infra/repository/labels.tf](../infra/repository/labels.tf). Do not refer to nonexistent templates.
Verify labels against the live repository with `gh label list` before an authorized
write; Terraform definitions do not prove the remote configuration was applied.
Usage questions belong in Discussions when appropriate.

`gh` does not apply form templates automatically. Prepare their required sections
in a temporary body file and pass it with `--body-file`; clean up the owned file
when finished. Re-read the created issue to verify its body, state, and labels.
Security vulnerabilities use private disclosure under [SECURITY.md](../SECURITY.md),
never a public issue template.

---

## Issue Title Format

### IT-1 (MUST): Clear and Descriptive

Issue titles MUST be:
- **Specific**: Clearly describe the problem or request
- **Concise**: Maximum 72 characters for readability
- **Uppercase Start**: Begin with uppercase letter
- **Professional**: Use technical language

**Examples:**

| Type | Example Title |
|------|---------------|
| Bug | `Bug: Reconciler panics when Deployment is missing namespace` |
| Feature | `Feature: Add KEDA ScaledObject support for autoscaling` |
| Performance | `Performance: Slow reconciliation loop under high pod count` |
| Documentation | `Docs: Missing migration guide for CRD v1alpha1 to v1beta1` |
| CI/CD | `CI: Integration tests failing on Kubernetes 1.29` |
| Security | `Security: Operator service account has overly broad RBAC permissions` |
| Question | `Question: How to configure custom reconciliation interval?` |

**Title Quality:**

- ❌ Bad: "Fix bug" (too vague)
- ❌ Bad: "performance issue" (unclear what)
- ❌ Bad: "add feature" (which feature?)
- ✅ Good: "Bug: Reconciler panics when Deployment is missing namespace"
- ✅ Good: "Feature: Add KEDA ScaledObject reconciliation support"

---

## Issue Labels

### IL-1 (MUST): Apply Type Labels

**ALL issues MUST have at least one type label:**

| Label | Color | Description |
|-------|-------|-------------|
| `bug` | #d73a4a | Confirmed bug or unexpected behavior |
| `enhancement` | #a2eeef | New feature or improvement request |
| `documentation` | #0075ca | Documentation issues or improvements |
| `question` | #d876e3 | Questions about usage or implementation |
| `performance` | #fbca04 | Performance-related issues |
| `ci-cd` | #2cbe4e | CI/CD workflow issues |
| `security` | #ee0701 | Security vulnerabilities or concerns |

### IL-2 (SHOULD): Apply Priority and Scope Labels

**Priority Labels:**

| Label | Color | Description |
|-------|-------|-------------|
| `critical` | #b60205 | Blocks release or major functionality |
| `high` | #d93f0b | Important fix or feature |
| `medium` | #fbca04 | Normal priority |
| `low` | #0e8a16 | Minor fix or enhancement |

**Scope Labels:**

| Label | Color | Description |
|-------|-------|-------------|
| `operator` | #ededed | Kubernetes operator logic |
| `crd` | #ededed | Custom Resource Definitions |
| `reconciler` | #ededed | Reconciliation loop |
| `rbac` | #ededed | RBAC roles and permissions |
| `database` | #ededed | Database layer |
| `api` | #ededed | API layer |

**Status Labels:**

| Label | Color | Description |
|-------|-------|-------------|
| `good first issue` | #7057ff | Suitable for new contributors |
| `help wanted` | #008672 | Community contributions welcome |
| `duplicate` | #cfd3d7 | Duplicate of another issue |
| `invalid` | #e4e669 | Not a valid issue |
| `wontfix` | #ffffff | Will not be fixed (intentional) |
| `needs more info` | #fef2c0 | Awaiting additional information |

### IL-3 (MUST): Agent-Detected Issue Labels

Issues created by LLM agent bug discovery MUST include the `agent-suspect` label:

| Label | Color | Description |
|-------|-------|-------------|
| `agent-suspect` | #d4c5f9 | Agent-detected issue pending independent verification |

**Rules:**
- ALL agent-detected issues MUST have `agent-suspect` label at creation
- The label is removed ONLY after independent verification confirms the issue
- Independent verification requires a separate agent (with independent context) or human review
- The verifying entity MUST NOT have participated in the initial detection
- This independent-verification rule does not authorize spawning another agent.
  Keep the label and report verification pending unless a human verifies it or
  the user explicitly requests independent delegated verification.

### IL-4 (MUST): Upstream Tracking Issue Labels

Tracking issues created for upstream dependency bugs MUST include the `upstream-tracking` label:

| Label | Color | Description |
|-------|-------|-------------|
| `upstream-tracking` | #c5def5 | Tracking issue for upstream dependency bugs (reinhardt-web etc.) |

**Rules:**
- ALL Reinhardt Cloud tracking issues for upstream bugs MUST have `upstream-tracking` label at creation
- The tracking issue title MUST follow the format: `Upstream: [brief description] (reinhardt-web#N)`
- The tracking issue MUST reference the upstream issue URL
- Close the tracking issue when the upstream issue is resolved AND any Reinhardt Cloud workaround is removed

See instructions/UPSTREAM_ISSUE_REPORTING.md (UR-4) for the full cross-referencing workflow.

---

## Issue Lifecycle

### LC-1 (MUST): Triage Process

**New Issues:**

1. **Automatic Labeling**: Issue template applies type label
2. **Maintainer Review**: Triage within 48 hours
3. **Label Enhancement**: Add priority and scope labels
4. **Assignment**: Assign to maintainer or contributor

The following diagram shows the issue lifecycle state transitions:

```mermaid
stateDiagram-v2
    [*] --> Open: Issue created
    Open --> Triaged: Maintainer adds priority/scope labels
    Triaged --> InProgress: Assigned and work started
    InProgress --> Blocked: Dependency or blocker
    Blocked --> InProgress: Blocker resolved
    InProgress --> Closed: Fixed (reference PR/commit)
    Open --> Closed: Invalid / Duplicate / Wontfix
    Triaged --> Closed: Wontfix with explanation
```

### LC-2 (MUST): Issue Hygiene

**Issue Closure:**

- **Fixed**: Close with comment describing fix and referencing PR/commit
- **Duplicate**: Close with reference to original issue
- **Wontfix**: Close with explanation of why it won't be fixed
- **Invalid**: Close with explanation

---

## Security Issues

### SEC-1 (MUST): Private Disclosure

**Security vulnerabilities MUST be reported privately:**

1. **DO NOT** create public issues for security vulnerabilities
2. **DO** use GitHub Security Advisories for private reporting

**How to Report:**

Via GitHub Security Advisories (Recommended):
```
https://github.com/kent8192/reinhardt-cloud/security/advisories
```

---

## Quick Reference

### ✅ MUST DO

- Write ALL issue content in English (no exceptions)
- Search existing issues before creating new ones
- Use appropriate issue templates for ALL issues
- Apply at least one type label to every issue
- Report security vulnerabilities privately via GitHub Security Advisories
- Provide minimal reproduction code for bug reports
- Include environment details (Rust version, Kubernetes version, OS)
- Be specific in issue titles (max 72 characters)
- Apply `agent-suspect` label to all agent-detected bug issues
- Verify agent-detected bugs independently before removing `agent-suspect` label
- Report upstream reinhardt-web issues immediately upon discovery (see instructions/UPSTREAM_ISSUE_REPORTING.md)

### ❌ NEVER DO

- Create public issues for security vulnerabilities
- Create duplicate issues without searching first
- Skip issue templates when creating issues
- Use non-English in issue titles or descriptions
- Create issues without appropriate labels
- Apply `release` label to issues (only for PRs)
- Submit bug reports without reproduction steps
- Leave issues inactive without response
- Remove `agent-suspect` label without independent verification

---

## Related Documentation

- **Pull Request Guidelines**: [PR_GUIDELINE.md](PR_GUIDELINE.md)
- **Issue Handling Principles**: [ISSUE_HANDLING.md](ISSUE_HANDLING.md)
- **Upstream Issue Reporting**: [UPSTREAM_ISSUE_REPORTING.md](UPSTREAM_ISSUE_REPORTING.md)
- **Commit Guidelines**: [COMMIT_GUIDELINE.md](COMMIT_GUIDELINE.md)
- **Security Policy**: [SECURITY.md](../SECURITY.md)
- **Label Definitions**: [infra/repository/labels.tf](../infra/repository/labels.tf)

---

**Note**: This document focuses on issue creation and management. For pull request guidelines, see instructions/PR_GUIDELINE.md.
