terraform {
  required_version = ">= 1.7"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.42"
    }
  }

  # Uncomment to use S3 + DynamoDB remote state backend.
  # backend "s3" {
  #   bucket         = "<your-terraform-state-bucket>"
  #   key            = "reinhardt-cloud/aws-minimal/terraform.tfstate"
  #   region         = "<your-region>"
  #   dynamodb_table = "<your-lock-table>"
  #   encrypt        = true
  # }
}

provider "aws" {
  region = var.region
}
