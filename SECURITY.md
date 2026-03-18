# Security Policy

## Supported Versions

| Version | Supported |
| --- | --- |
| Latest on `feature/platform-basis` | :white_check_mark: |
| Older releases | :x: |

## Reporting a Vulnerability

**Please do NOT report security vulnerabilities through public GitHub issues.**

Instead, please report them via [GitHub Security Advisories](https://github.com/kent8192/nuages/security/advisories/new).

### What to Include

When reporting a vulnerability, please include:

- A description of the vulnerability
- Steps to reproduce the issue
- Potential impact of the vulnerability
- Any suggested fixes (if available)

### Response Timeline

- **Acknowledgment**: Within 48 hours of report submission
- **Triage**: Within 7 days, we will assess severity and impact
- **Fix**: Critical vulnerabilities will be addressed within 30 days

### Severity Classification

| Severity | Description | Target Resolution |
| --- | --- | --- |
| Critical | Remote code execution, authentication bypass, data exposure | 30 days |
| High | Privilege escalation, significant data leaks | 60 days |
| Medium | Limited impact vulnerabilities | 90 days |
| Low | Minor issues, hardening suggestions | Next release |

## Scope

The following are in scope for security reports:

- The Nuages application and its dependencies
- Authentication and authorization mechanisms
- API endpoint security
- Kubernetes operator security (CRD handling, RBAC)
- CI/CD pipeline security

The following are out of scope:

- Third-party services not maintained by this project
- Issues in upstream dependencies (report to the respective project)
- Social engineering attacks

## Safe Harbor

We support safe harbor for security researchers who:

- Make a good faith effort to avoid privacy violations, data destruction, and service disruption
- Only interact with accounts you own or with explicit permission from the account holder
- Do not exploit a security issue for purposes beyond what is necessary to demonstrate the vulnerability
- Report vulnerabilities promptly and provide sufficient detail

We will not pursue legal action against researchers who follow these guidelines.

## Contact

For security-related inquiries, use [GitHub Security Advisories](https://github.com/kent8192/nuages/security/advisories/new).
