output "secret_arn" {
  description = "The ARN of the Secrets Manager secret."
  value       = aws_secretsmanager_secret.app.arn
}

output "secret_name" {
  description = "The full name of the Secrets Manager secret."
  value       = aws_secretsmanager_secret.app.name
}
