output "repository_url" {
  description = "The URL of the ECR repository (account.dkr.ecr.region.amazonaws.com/name)."
  value       = aws_ecr_repository.images.repository_url
}

output "repository_arn" {
  description = "The ARN of the ECR repository."
  value       = aws_ecr_repository.images.arn
}

output "repository_name" {
  description = "The short name of the ECR repository."
  value       = aws_ecr_repository.images.name
}
