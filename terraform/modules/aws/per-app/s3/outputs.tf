output "bucket_name" {
  description = "The full name of the S3 bucket."
  value       = aws_s3_bucket.app.bucket
}

output "bucket_arn" {
  description = "The ARN of the S3 bucket."
  value       = aws_s3_bucket.app.arn
}
