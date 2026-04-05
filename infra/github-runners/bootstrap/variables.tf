variable "aws_region" {
	description = "AWS region"
	type        = string
	default     = "us-east-1"
}

variable "aws_account_id" {
	description = "AWS Account ID. Used to construct globally unique S3 state bucket name (nuages-ci-terraform-state-<account_id>)."
	type        = string
}
