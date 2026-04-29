resource "aws_s3_bucket" "app" {
  bucket = "${var.name_prefix}-${var.bucket_name}"
  tags   = var.tags
}

resource "aws_s3_bucket_server_side_encryption_configuration" "app" {
  bucket = aws_s3_bucket.app.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}

resource "aws_s3_bucket_versioning" "app" {
  bucket = aws_s3_bucket.app.id

  versioning_configuration {
    status = "Enabled"
  }
}

resource "aws_s3_bucket_public_access_block" "app" {
  bucket = aws_s3_bucket.app.id

  # When public = false (default), block all public access.
  # When public = true, allow public reads for static asset hosting.
  block_public_acls       = !var.public
  block_public_policy     = !var.public
  ignore_public_acls      = !var.public
  restrict_public_buckets = !var.public
}
