# Cancel runner infrastructure for Nuages.
# Manages an always-on EC2 instance that runs GitHub Actions cancel workflows.

terraform {
	required_version = ">= 1.10"

	required_providers {
		aws = {
			source  = "hashicorp/aws"
			version = "~> 5.0"
		}
	}

	# Partial backend: supply values via -backend-config=backend.tfvars
	# See init.sh for automatic generation
	backend "s3" {
		use_lockfile = true
	}
}

provider "aws" {
	region = var.aws_region

	default_tags {
		tags = {
			Project   = "nuages"
			ManagedBy = "terraform"
			Component = "github-runners"
		}
	}
}

# Default VPC data sources (use default VPC for simplicity)
data "aws_vpc" "default" {
	default = true
}

data "aws_subnets" "default" {
	filter {
		name   = "vpc-id"
		values = [data.aws_vpc.default.id]
	}
	filter {
		name   = "map-public-ip-on-launch"
		values = ["true"]
	}
}

# Latest Ubuntu 24.04 LTS ARM64 AMI
data "aws_ami" "ubuntu_arm64_latest" {
	most_recent = true
	owners      = ["099720109477"] # Canonical

	filter {
		name   = "name"
		values = ["ubuntu/images/hvm-ssd-gp3/ubuntu-noble-24.04-arm64-server-*"]
	}

	filter {
		name   = "virtualization-type"
		values = ["hvm"]
	}

	filter {
		name   = "architecture"
		values = ["arm64"]
	}
}
