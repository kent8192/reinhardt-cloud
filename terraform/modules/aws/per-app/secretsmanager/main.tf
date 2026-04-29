resource "aws_secretsmanager_secret" "app" {
  name        = "${var.name_prefix}-${var.secret_name}"
  description = var.description
  # Use a customer-managed KMS key when provided for enhanced encryption control.
  kms_key_id              = var.kms_key_id
  recovery_window_in_days = var.recovery_window_days

  tags = var.tags
}
