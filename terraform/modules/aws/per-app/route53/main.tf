resource "aws_route53_record" "app" {
  zone_id = var.zone_id
  name    = var.host
  type    = var.record_type
  ttl     = var.ttl
  records = var.records
}
