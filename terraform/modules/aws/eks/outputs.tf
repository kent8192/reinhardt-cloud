output "cluster_name" {
  description = "The name of the EKS cluster."
  value       = aws_eks_cluster.primary.name
}

output "cluster_endpoint" {
  description = "The endpoint URL for the EKS cluster API server."
  value       = aws_eks_cluster.primary.endpoint
  sensitive   = true
}

output "cluster_ca" {
  description = "The cluster CA certificate data (base64-encoded) for kubeconfig."
  value       = aws_eks_cluster.primary.certificate_authority[0].data
  sensitive   = true
}

output "oidc_issuer_url" {
  description = "The OIDC issuer URL for the EKS cluster (used for IRSA)."
  value       = aws_eks_cluster.primary.identity[0].oidc[0].issuer
}

output "node_role_arn" {
  description = "The ARN of the IAM role used by EKS managed node groups."
  value       = aws_iam_role.nodes.arn
}
