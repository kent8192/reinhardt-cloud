# Nuages

A Kubernetes-native PaaS for deploying [Reinhardt](https://github.com/kent8192/reinhardt-web) web applications.

Named after Django Reinhardt's composition *Nuages* (French: "Clouds").

## Overview

Nuages implements convention-driven deployment: the CLI analyzes a Reinhardt project's
structure and generates deployment manifests automatically, so developers focus on code
rather than infrastructure configuration.

## Architecture

Three-plane architecture inspired by Vercel:

- **Control Plane** (`nuages-control-plane`) — REST API, authentication, project management
- **Operator** (`nuages-operator`) — Kubernetes Operator that reconciles `ReinhardtApp` CRDs
- **CLI** (`nuages-cli`) — `nuages-deploy` command for project analysis and deployment

See [docs/2026-03-14-reinhardt-paas-design.md](docs/2026-03-14-reinhardt-paas-design.md) for the full design.

## Quick Start

```bash
# In your Reinhardt project
nuages-deploy init    # Analyze project, generate reinhardt.toml
nuages-deploy deploy  # Build and deploy to the platform
nuages-deploy logs    # Stream application logs
```

## License

Business Source License 1.1
