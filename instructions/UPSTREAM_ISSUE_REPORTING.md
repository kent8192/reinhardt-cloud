# Upstream Issue Reporting

## Purpose

This file defines the policy for reporting issues discovered in upstream dependencies during Reinhardt Cloud development. Reinhardt Cloud is a PaaS built on top of [reinhardt-web](https://github.com/kent8192/reinhardt-web), and issues found in reinhardt-web during Reinhardt Cloud work MUST be reported immediately to the upstream repository.

---

## Scope

### US-1 (MUST): Target Repositories

This policy applies to the following upstream repositories:

| Repository | URL | Relationship |
|------------|-----|-------------|
| reinhardt-web | `https://github.com/kent8192/reinhardt-web` | Core web framework that Reinhardt Cloud deploys |

**Future upstream dependencies** should be added to this table as the project grows.

---

## Reporting Policy

### UR-1 (MUST): Immediate Reporting

When a bug, missing feature, documentation gap, or unexpected behavior in reinhardt-web is discovered during Reinhardt Cloud development, an issue MUST be created in the reinhardt-web repository **immediately** upon discovery.

**Rationale:** Delaying upstream issue reporting increases the risk of:
- Forgetting the issue details
- Building workarounds that mask the root cause
- Other contributors hitting the same problem without context

The following diagram summarizes the upstream issue reporting flow:

```mermaid
flowchart TD
    A[Discover issue during Reinhardt Cloud development] --> B{Is the issue in Reinhardt Cloud code?}
    B -->|Yes| C[Create issue in reinhardt-cloud repo]
    B -->|No| D{Is the issue in reinhardt-web?}
    D -->|Yes| E[Create issue in reinhardt-web repo immediately]
    D -->|No| F[Investigate further]
    E --> E2["Create tracking issue in reinhardt-cloud repo<br/>with upstream-tracking label"]
    E2 --> E3["Cross-reference both issues"]
    E3 --> G{Does Reinhardt Cloud need a workaround?}
    G -->|Yes| H["Add workaround in Reinhardt Cloud with<br/>comment referencing both issues"]
    G -->|No| I[Continue Reinhardt Cloud development]
    H --> I
```

### UR-2 (MUST): Use GitHub CLI with Repository Flag

Issues in upstream repositories MUST be created using `gh issue create` with the `-R` flag:

```bash
# Create issue in reinhardt-web
gh issue create -R kent8192/reinhardt-web \
  --title "Bug: description of the issue" \
  --body "$(cat <<'EOF'
## Description

[Clear description of the issue]

## Reproduction Steps

1. [Step 1]
2. [Step 2]

## Expected Behavior

[What should happen]

## Actual Behavior

[What actually happens]

## Context

Discovered during Reinhardt Cloud development while [brief context].

Reinhardt Cloud tracking issue: https://github.com/kent8192/reinhardt-cloud/issues/N

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

### UR-3 (MUST): Issue Content Requirements

Upstream issues MUST:
- Be written in **English**
- Follow the upstream repository's issue templates if available
- Include clear reproduction steps
- Include the discovery context (e.g., "discovered during Reinhardt Cloud operator reconciler implementation")
- Reference related Reinhardt Cloud issues or PRs if applicable
- Include Claude Code attribution footer
- **NOT** include absolute local paths or user-specific information

### UR-4 (MUST): Cross-Referencing with Tracking Issues

When an upstream issue is created, a corresponding **tracking issue** MUST also be created in the Reinhardt Cloud repository. Both issues MUST reference each other.

**Rationale:** Creating a tracking issue in Reinhardt Cloud ensures that:
- Upstream dependencies are visible in the Reinhardt Cloud issue tracker
- Workaround removal can be tracked alongside Reinhardt Cloud development milestones
- Contributors can discover upstream blockers without checking external repositories

**Procedure:**

1. **Create the upstream issue** in reinhardt-web (UR-1, UR-2)
2. **Create a tracking issue** in the Reinhardt Cloud repository referencing the upstream issue
3. **Update the upstream issue** to reference the Reinhardt Cloud tracking issue
4. **In the Reinhardt Cloud codebase**: Add a comment referencing both issues where a workaround is applied

The following diagram shows the cross-referencing workflow:

```mermaid
sequenceDiagram
    participant D as Developer
    participant RW as reinhardt-web
    participant N as Reinhardt Cloud

    D->>RW: 1. Create upstream issue (reinhardt-web#42)
    D->>N: 2. Create tracking issue (reinhardt-cloud#15)<br/>References reinhardt-web#42
    D->>RW: 3. Update reinhardt-web#42<br/>Add link to reinhardt-cloud#15
    D->>N: 4. Add workaround code comment<br/>referencing both issues
```

**Reinhardt Cloud tracking issue template:**

```bash
gh issue create \
  --title "Upstream: [brief description] (reinhardt-web#N)" \
  --label upstream-tracking \
  --body "$(cat <<'EOF'
## Upstream Issue

Tracking upstream issue: https://github.com/kent8192/reinhardt-web/issues/N

## Impact on Reinhardt Cloud

[Describe how this upstream issue affects Reinhardt Cloud]

## Workaround

- [ ] Workaround applied in Reinhardt Cloud (if needed)
- [ ] Code comment added referencing upstream issue

## Resolution Criteria

This issue should be closed when:
- The upstream issue is resolved AND
- The Reinhardt Cloud workaround (if any) is removed

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

**Workaround comment format:**
```rust
// Workaround for kent8192/reinhardt-web#42 (tracked in reinhardt-cloud#15)
// Remove this workaround when the upstream issue is resolved.
```

### UR-5 (SHOULD): Label Application

Apply appropriate labels to upstream issues based on the issue type:

| Issue Type | Labels |
|------------|--------|
| Bug | `bug` |
| Missing feature | `enhancement` |
| Documentation gap | `documentation` |
| Performance issue | `performance` |

**Note:** Available labels depend on the upstream repository's configuration. Check available labels before applying.

---

## Issue Categories

### IC-1: What Qualifies as an Upstream Issue

Report to the upstream repository when:

- A reinhardt-web API behaves unexpectedly or inconsistently
- A reinhardt-web feature is missing that Reinhardt Cloud requires
- reinhardt-web documentation is incorrect, incomplete, or misleading
- A reinhardt-web dependency causes a conflict or vulnerability
- reinhardt-web build or test infrastructure has issues that affect downstream consumers
- reinhardt-web type signatures or trait implementations are incorrect

### IC-2: What Does NOT Qualify

Do **NOT** report to the upstream repository when:

- The issue is in Reinhardt Cloud-specific code (report in Reinhardt Cloud repo)
- The issue is a Reinhardt Cloud design decision that differs from reinhardt-web conventions
- The issue is a feature request specific to Reinhardt Cloud's PaaS use case with no general applicability
- The issue is a misunderstanding of reinhardt-web's intended behavior (use Discussions instead)

---

## Workaround Policy

### WP-1 (SHOULD): Temporary Workarounds

When an upstream issue blocks Reinhardt Cloud development:

1. Create the upstream issue first (UR-1)
2. Create the Reinhardt Cloud tracking issue with `upstream-tracking` label (UR-4)
3. Cross-reference both issues (UR-4)
4. Implement a minimal workaround in Reinhardt Cloud
5. Mark the workaround with a comment referencing both issues (UR-4)
6. Track the upstream issue for resolution; close the Reinhardt Cloud tracking issue when resolved

**Workaround rules:**
- Keep workarounds minimal and isolated
- Document the workaround clearly
- Remove the workaround when the upstream issue is resolved

### WP-2 (MUST): No Silent Workarounds

**NEVER** implement workarounds for upstream issues without:
1. Creating an upstream issue first
2. Adding a reference comment in the workaround code

---

## Quick Reference

### ✅ MUST DO
- Create issues in reinhardt-web immediately upon discovering upstream bugs (UR-1)
- Use `gh issue create -R kent8192/reinhardt-web` for upstream issue creation (UR-2)
- Write all upstream issues in English (UR-3)
- Follow upstream repository's issue templates when available (UR-3)
- Create a tracking issue in Reinhardt Cloud for every upstream issue with `upstream-tracking` label (UR-4)
- Cross-reference between Reinhardt Cloud tracking issue and upstream issue bidirectionally (UR-4)
- Add workaround comments referencing both upstream and Reinhardt Cloud tracking issues (UR-4)
- Create upstream issue before implementing any workaround (WP-2)

### ❌ NEVER DO
- Delay reporting upstream issues discovered during Reinhardt Cloud development
- Implement workarounds without creating upstream issues first (WP-2)
- Create upstream issues without corresponding Reinhardt Cloud tracking issues (UR-4)
- Include absolute local paths in upstream issues (UR-3)
- Report Reinhardt Cloud-specific issues to the reinhardt-web repository (IC-2)
- Forget to cross-reference between Reinhardt Cloud and upstream issues (UR-4)

---

## Related Documentation

- **Issue Guidelines**: instructions/ISSUE_GUIDELINES.md
- **Issue Handling**: instructions/ISSUE_HANDLING.md
- **GitHub Interaction**: instructions/GITHUB_INTERACTION.md
- **Main Quick Reference**: CLAUDE.md (see Quick Reference section)
- **reinhardt-web Repository**: <https://github.com/kent8192/reinhardt-web>

---

**Note**: This document focuses on reporting issues to upstream dependencies. For Reinhardt Cloud-specific issue management, see instructions/ISSUE_GUIDELINES.md. For batch issue handling strategy, see instructions/ISSUE_HANDLING.md.
