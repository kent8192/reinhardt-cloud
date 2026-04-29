output "cluster_endpoint" {
  description = "EKS cluster API endpoint for kubeconfig."
  value       = module.eks.cluster_endpoint
  sensitive   = true
}

output "cluster_ca" {
  description = "EKS cluster CA certificate (base64) for kubeconfig."
  value       = module.eks.cluster_ca
  sensitive   = true
}

output "rds_endpoint" {
  description = "RDS endpoint for the dashboard DATABASE_URL."
  value       = module.rds.endpoint
  sensitive   = true
}

output "ecr_repository_url" {
  description = "ECR repository URL for container image push/pull."
  value       = module.ecr.repository_url
}

output "operator_iam_role_arn" {
  description = "Operator IRSA role ARN for the Kubernetes service account annotation."
  value       = module.irsa.operator_iam_role_arn
}

output "dashboard_iam_role_arn" {
  description = "Dashboard IRSA role ARN for the Kubernetes service account annotation."
  value       = module.irsa.dashboard_iam_role_arn
}
