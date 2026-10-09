# Reinhardt Cloud Control Plane

The Control Plane is the Reinhardt application that holds Reinhardt Cloud's platform state and serves it to people, to the CLI, and to cluster agents. It is also the canonical dogfooding target of the configuration-to-deployment pipeline.

## Language

### Application surfaces

**Control Plane**:
The single Reinhardt application that owns platform state and exposes every surface listed below.
_Avoid_: dashboard (when meaning the whole application), backend, API server

**Dashboard**:
The browser-facing user interface of the Control Plane.
_Avoid_: frontend, web UI, console, SPA

**Agent Gateway**:
The surface of the Control Plane that cluster agents connect to in order to receive commands and report state.
_Avoid_: gRPC server, agent API

### People and access

**User**:
A person known to the Control Plane, identified by their GitHub account.
_Avoid_: account, login, customer

**Member**:
A User's participation in one Organization, carrying exactly one role (owner, admin, developer, or viewer).
_Avoid_: membership (in prose), collaborator, seat

**Invitation**:
A pending offer, addressed to a GitHub account, to become a Member of an Organization; it is fulfilled when that account first signs in.
_Avoid_: invite (as a noun), request

**Staff**:
A User trusted to operate the Control Plane itself, across all Organizations.
_Avoid_: admin (which is an Organization role), operator, superuser (in prose)

**API Key**:
A long-lived credential a User issues for the CLI or CI to act on their behalf.
_Avoid_: token, personal access token, PAT

**Login Link**:
A single-use, short-lived sign-in URL that only someone with operator access to the Control Plane's host can issue, used for automation and break-glass access.
_Avoid_: magic link, backdoor

### Platform model

**Organization**:
The tenant of Reinhardt Cloud: the unit that owns Projects and Clusters and grants roles to its members.
_Avoid_: tenant (except when naming the per-Organization cluster namespace), team, account, workspace

**Cluster**:
A Kubernetes cluster registered to an Organization, which the Control Plane reaches only through its Agent.
_Avoid_: environment, region

**Agent**:
The program running inside a Cluster that executes Control Plane commands and reports the Cluster's state back.
_Avoid_: cluster agent, worker

**Project**:
A Reinhardt application owned by an Organization, with a stable name and a source (a GitHub repository or manual submission).
_Avoid_: app, application, service, GitHub project

**Deployment**:
One attempt to put a Project's desired state onto a Cluster; its content never changes after submission, only its status advances.
_Avoid_: release, rollout, deploy (as a noun)

**Preview**:
A short-lived instance of a Project for a pull request, created inside a Cluster and only observed by the Control Plane.
_Avoid_: preview deployment, preview environment, review app
