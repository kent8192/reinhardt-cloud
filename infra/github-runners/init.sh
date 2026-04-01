#!/bin/bash
# init.sh: Generate backend.tfvars from terraform.tfvars and initialize Terraform.
#
# Usage:
#   cd infra/github-runners
#   cp terraform.example.tfvars terraform.tfvars
#   # Edit terraform.tfvars (set aws_account_id, github_app_id, etc.)
#   ./init.sh
#
# What this script does:
#   1. Reads aws_account_id and aws_region from terraform.tfvars
#   2. Generates backend.tfvars with the correct S3 bucket name
#   3. Runs terraform init -backend-config=backend.tfvars

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

TFVARS_FILE="terraform.tfvars"
BACKEND_FILE="backend.tfvars"
STATE_KEY="github-runners/terraform.tfstate"

# Verify terraform.tfvars exists
if [[ ! -f "$TFVARS_FILE" ]]; then
  echo "ERROR: $TFVARS_FILE not found."
  echo "       Copy terraform.example.tfvars to terraform.tfvars and fill in the values."
  exit 1
fi

# Extract a quoted string value from terraform.tfvars.
# Expects format: variable_name = "value"
extract_tfvar() {
  local var_name="$1"
  local value
  value=$(grep -E "^[[:space:]]*${var_name}[[:space:]]*=" "$TFVARS_FILE" \
    | sed -E 's/^[^"]*"([^"]*)".*$/\1/')
  if [[ "$value" == *"="* || -z "$value" ]]; then
    echo ""
  else
    echo "$value"
  fi
}

# Extract aws_account_id (required - no default)
ACCOUNT_ID=$(extract_tfvar "aws_account_id")
if [[ -z "$ACCOUNT_ID" ]]; then
  echo "ERROR: aws_account_id is not set in $TFVARS_FILE"
  echo "       Expected format: aws_account_id = \"123456789012\""
  exit 1
fi

# Extract aws_region (optional - default us-east-1)
REGION=$(extract_tfvar "aws_region")
REGION="${REGION:-us-east-1}"

BUCKET_NAME="nuages-ci-terraform-state-${ACCOUNT_ID}"

# Generate backend.tfvars
cat > "$BACKEND_FILE" <<EOF
bucket       = "${BUCKET_NAME}"
key          = "${STATE_KEY}"
region       = "${REGION}"
use_lockfile = true
EOF

echo "Generated ${BACKEND_FILE}:"
echo "  bucket       = \"${BUCKET_NAME}\""
echo "  key          = \"${STATE_KEY}\""
echo "  region       = \"${REGION}\""
echo "  use_lockfile = true"
echo ""

# Run terraform init with generated backend config
terraform init -backend-config="$BACKEND_FILE"
echo ""
echo "Next step: terraform plan"
