output "operator_iam_role_arn" {
  description = "The ARN of the operator IRSA role annotated on the Kubernetes service account."
  value       = aws_iam_role.operator.arn
}

output "dashboard_iam_role_arn" {
  description = "The ARN of the dashboard IRSA role annotated on the Kubernetes service account."
  value       = aws_iam_role.dashboard.arn
}
