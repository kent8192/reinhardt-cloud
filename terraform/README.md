# Reinhardt Cloud — Terraform IaC

This directory contains Terraform modules and examples for bootstrapping and operating Reinhardt Cloud on GCP and AWS.

## Structure

```
terraform/
├── modules/
│   ├── gcp/                        # GCP cluster bootstrap modules
│   │   ├── network/                # VPC, subnets, NAT, firewall
│   │   ├── gke/                    # GKE cluster + managed node pool
│   │   ├── cloudsql/               # Cloud SQL (Postgres) + IAM auth
│   │   ├── artifact_registry/      # Artifact Registry (Docker)
│   │   ├── workload_identity/      # GSAs + Workload Identity bindings
│   │   └── per-app/                # Per-app managed resources
│   │       ├── cloudsql/           # Per-app Cloud SQL instance
│   │       ├── gcs/                # GCS bucket
│   │       ├── dns/                # Cloud DNS record
│   │       └── secrets/            # Secret Manager secret
│   └── aws/                        # AWS cluster bootstrap modules
│       ├── network/                # VPC, subnets, NAT
│       ├── eks/                    # EKS cluster + managed node group
│       ├── rds/                    # RDS Postgres
│       ├── ecr/                    # ECR repository
│       ├── irsa/                   # OIDC provider + IRSA roles
│       └── per-app/                # Per-app managed resources
│           ├── rds/                # Per-app RDS instance
│           ├── s3/                 # S3 bucket
│           ├── route53/            # Route 53 record
│           └── secretsmanager/     # Secrets Manager secret
└── examples/
    ├── gcp-minimal/                # Single-zone GCP cluster example
    └── aws-minimal/                # Single-AZ AWS cluster example
```

## Quick Start — GCP

### Prerequisites

- Terraform >= 1.7
- `gcloud` CLI authenticated with a project owner or editor role
- GCP APIs enabled: `container.googleapis.com`, `sqladmin.googleapis.com`, `artifactregistry.googleapis.com`, `secretmanager.googleapis.com`

### Bootstrap

```bash
cd terraform/examples/gcp-minimal

# Copy and edit variables
cp terraform.tfvars.example terraform.tfvars
# Edit terraform.tfvars: set project_id, region, zone, name_prefix

# Initialise and apply
terraform init
terraform plan
terraform apply

# Capture outputs for Helm
terraform output artifact_registry_url
terraform output cloudsql_connection_name
terraform output operator_sa_email
terraform output dashboard_sa_email
```

### Helm install

```bash
helm install reinhardt-cloud charts/reinhardt-cloud-operator \
  -f charts/reinhardt-cloud-operator/values-gcp.yaml \
  --set image.repository=$(terraform -chdir=terraform/examples/gcp-minimal output -raw artifact_registry_url)/reinhardt-cloud-operator \
  --set serviceAccount.annotations."iam\.gke\.io/gcp-service-account"=$(terraform -chdir=terraform/examples/gcp-minimal output -raw operator_sa_email)
```

## Quick Start — AWS

### Prerequisites

- Terraform >= 1.7
- AWS CLI configured with sufficient IAM permissions
- AWS services available: EKS, RDS, ECR, Secrets Manager

### Bootstrap

```bash
cd terraform/examples/aws-minimal

# Copy and edit variables
cp terraform.tfvars.example terraform.tfvars
# Edit terraform.tfvars: set region, name_prefix, availability_zones
# Supply db_password via environment variable (never hardcode):
export TF_VAR_db_password="$(openssl rand -base64 24)"

# Initialise and apply
terraform init
terraform plan
terraform apply

# Capture outputs for Helm
terraform output ecr_repository_url
terraform output rds_endpoint
terraform output operator_iam_role_arn
terraform output dashboard_iam_role_arn
```

### Helm install

```bash
helm install reinhardt-cloud charts/reinhardt-cloud-operator \
  -f charts/reinhardt-cloud-operator/values-aws.yaml \
  --set image.repository=$(terraform -chdir=terraform/examples/aws-minimal output -raw ecr_repository_url) \
  --set serviceAccount.annotations."eks\.amazonaws\.com/role-arn"=$(terraform -chdir=terraform/examples/aws-minimal output -raw operator_iam_role_arn)
```

## Per-App Infrastructure (Issue #420)

Once an application's `ReinhardtApp` CRD has an `infrastructure:` block, use the CLI to generate per-app HCL:

```bash
reinhardt-cloud terraform generate \
  --app orders-api \
  --provider gcp \
  --output ./orders-api-terraform

cd orders-api-terraform
terraform init
terraform apply
```

Example `ReinhardtApp` with infrastructure declaration:

```yaml
apiVersion: paas.reinhardt-cloud.dev/v1alpha2
kind: ReinhardtApp
metadata:
  name: orders-api
  namespace: tenant-acme
spec:
  image: us-central1-docker.pkg.dev/my-project/reinhardt/orders-api:v1.2.3
  replicas: 3
  infrastructure:
    postgres:
      tier: db-custom-2-4096
      version: "16"
      backupRetentionDays: 7
    buckets:
      - name: order-uploads
        public: false
    secrets:
      - name: stripe-key
        description: Stripe API key for payment processing
```

## Remote State

Uncomment and configure the `backend` block in each example's `versions.tf`:

**GCP (GCS):**

```hcl
backend "gcs" {
  bucket = "my-terraform-state"
  prefix = "reinhardt-cloud/gcp-minimal"
}
```

**AWS (S3 + DynamoDB):**

```hcl
backend "s3" {
  bucket         = "my-terraform-state"
  key            = "reinhardt-cloud/aws-minimal/terraform.tfstate"
  region         = "us-east-1"
  dynamodb_table = "terraform-locks"
  encrypt        = true
}
```

## On-Premise (Future Work — Phase 4)

On-premise support (k3s/kubeadm, local Postgres, self-hosted Harbor) is planned as a follow-up phase. The `values-onprem.yaml` Helm overlay already exists; the Terraform layer will be added once the GCP and AWS tracks stabilise. Contributions welcome — see [GitHub Discussions](https://github.com/kent8192/reinhardt-cloud/discussions) to discuss the design before submitting a PR.

## Security Notes

- **No credentials are hardcoded.** All sensitive values (database passwords, service account keys) are passed via Terraform variables or retrieved from the provider's secret manager.
- **EKS API server** is private-endpoint-only (`endpoint_public_access = false`). Access via VPN or AWS Systems Manager Session Manager.
- **RDS and Cloud SQL** are deployed with private IP only; no public endpoint.
- **S3 buckets** block all public access by default unless `public = true` is explicitly set.
- **Secrets Manager** accepts an optional `kms_key_id` for customer-managed key encryption.
