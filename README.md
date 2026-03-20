# Reinhardt Cloud

A Kubernetes-native PaaS for deploying [Reinhardt](https://github.com/kent8192/reinhardt-web) web applications.

Named after Django Reinhardt's composition *Nuages* (French: "Clouds").

## Overview

Reinhardt Cloud implements convention-driven deployment: the CLI analyzes a Reinhardt project's
structure and generates deployment manifests automatically, so developers focus on code
rather than infrastructure configuration.

## Architecture

Three-plane architecture inspired by Vercel:

- **Control Plane** (`app/`) — reinhardt-web REST API, authentication, project management
- **Operator** (`reinhardt-cloud-operator`) — Kubernetes Operator that reconciles `ReinhardtApp` CRDs
- **CLI** (`reinhardt-cloud-cli`) — `reinhardt-cloud` command for deployment and management

## Quick Start

> **Note:** These CLI subcommands are currently scaffolded; full implementation is in progress.

```bash
reinhardt-cloud login --username alice                    # Authenticate with the platform
reinhardt-cloud deploy --name myapp --image myapp:v1      # Deploy to the platform
reinhardt-cloud status --name myapp                       # Check deployment status
```

## License

Business Source License 1.1
