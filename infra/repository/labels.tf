# Repository issue/PR labels managed by Terraform.
# See .github/labels.yml for documentation of label semantics.

locals {
  labels = {
    # --- Type labels ---
    "bug" = {
      color       = "d73a4a"
      description = "Confirmed bug or unexpected behavior"
    }
    "enhancement" = {
      color       = "a2eeef"
      description = "New feature or improvement request"
    }
    "documentation" = {
      color       = "0075ca"
      description = "Documentation issues or improvements"
    }
    "question" = {
      color       = "d876e3"
      description = "Questions about usage or implementation"
    }
    "performance" = {
      color       = "fbca04"
      description = "Performance-related issues"
    }
    "ci-cd" = {
      color       = "2cbe4e"
      description = "CI/CD workflow issues"
    }
    "security" = {
      color       = "ee0701"
      description = "Security vulnerabilities or concerns"
    }
    "refactoring" = {
      color       = "e4e669"
      description = "Code refactoring without behavior change"
    }
    "code-quality" = {
      color       = "bfd4f2"
      description = "Code quality improvements (tests, linting)"
    }
    "breaking-change" = {
      color       = "b60205"
      description = "Breaking API or behavior change"
    }
    "dependencies" = {
      color       = "0366d6"
      description = "Dependency version updates"
    }

    # --- Priority labels ---
    "critical" = {
      color       = "b60205"
      description = "Blocks release or major functionality"
    }
    "high" = {
      color       = "d93f0b"
      description = "Important fix or feature"
    }
    "medium" = {
      color       = "fbca04"
      description = "Normal priority"
    }
    "low" = {
      color       = "0e8a16"
      description = "Minor fix or enhancement"
    }

    # --- Scope labels (reinhardt-cloud-specific) ---
    "api" = {
      color       = "ededed"
      description = "reinhardt-cloud-api crate changes"
    }
    "k8s" = {
      color       = "ededed"
      description = "reinhardt-cloud-k8s crate / Kubernetes resources"
    }
    "types" = {
      color       = "ededed"
      description = "reinhardt-cloud-types crate changes"
    }
    "cli" = {
      color       = "ededed"
      description = "reinhardt-cloud-cli crate changes"
    }
    "operator" = {
      color       = "ededed"
      description = "reinhardt-cloud-operator crate / Kubernetes operator"
    }
    "control" = {
      color       = "ededed"
      description = "reinhardt-cloud-control crate changes"
    }
    "infra" = {
      color       = "ededed"
      description = "Infrastructure and Terraform changes"
    }

    # --- Status labels ---
    "good first issue" = {
      color       = "7057ff"
      description = "Suitable for new contributors"
    }
    "help wanted" = {
      color       = "008672"
      description = "Community contributions welcome"
    }
    "duplicate" = {
      color       = "cfd3d7"
      description = "Duplicate of another issue"
    }
    "invalid" = {
      color       = "e4e669"
      description = "Not a valid issue"
    }
    "wontfix" = {
      color       = "ffffff"
      description = "Will not be fixed (intentional)"
    }
    "needs more info" = {
      color       = "fef2c0"
      description = "Awaiting additional information"
    }
    "auto-closed" = {
      color       = "cfd3d7"
      description = "Automatically closed when all sub-issues completed"
    }
    "no-auto-close" = {
      color       = "fef2c0"
      description = "Prevents automatic closure when sub-issues complete"
    }

    # --- Workflow labels ---
    "release" = {
      color       = "0e8a16"
      description = "Release preparation PR (triggers release automation)"
    }
    "migration-approved" = {
      color       = "0075ca"
      description = "Approved version transition from develop/* branch"
    }
    "agent-suspect" = {
      color       = "d4c5f9"
      description = "Agent-detected issue pending independent verification"
    }
    "rc-addition" = {
      color       = "fbca04"
      description = "Non-breaking API addition during RC phase (requires approval)"
    }
  }
}

resource "github_issue_label" "labels" {
  for_each = local.labels

  repository  = var.repository_name
  name        = each.key
  color       = each.value.color
  description = each.value.description
}
