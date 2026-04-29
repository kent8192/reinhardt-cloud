data "aws_caller_identity" "current" {}
data "aws_region" "current" {}

locals {
  oidc_provider = replace(var.oidc_issuer_url, "https://", "")
}

data "aws_iam_openid_connect_provider" "eks" {
  url = var.oidc_issuer_url
}

data "aws_iam_policy_document" "operator_assume_role" {
  statement {
    actions = ["sts:AssumeRoleWithWebIdentity"]
    principals {
      type        = "Federated"
      identifiers = [data.aws_iam_openid_connect_provider.eks.arn]
    }
    condition {
      test     = "StringEquals"
      variable = "${local.oidc_provider}:sub"
      values   = ["system:serviceaccount:${var.operator_namespace}:${var.operator_ksa_name}"]
    }
    condition {
      test     = "StringEquals"
      variable = "${local.oidc_provider}:aud"
      values   = ["sts.amazonaws.com"]
    }
  }
}

data "aws_iam_policy_document" "dashboard_assume_role" {
  statement {
    actions = ["sts:AssumeRoleWithWebIdentity"]
    principals {
      type        = "Federated"
      identifiers = [data.aws_iam_openid_connect_provider.eks.arn]
    }
    condition {
      test     = "StringEquals"
      variable = "${local.oidc_provider}:sub"
      values   = ["system:serviceaccount:${var.dashboard_namespace}:${var.dashboard_ksa_name}"]
    }
    condition {
      test     = "StringEquals"
      variable = "${local.oidc_provider}:aud"
      values   = ["sts.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "operator" {
  name               = "${var.name_prefix}-operator-irsa"
  assume_role_policy = data.aws_iam_policy_document.operator_assume_role.json
  tags               = var.tags
}

resource "aws_iam_role" "dashboard" {
  name               = "${var.name_prefix}-dashboard-irsa"
  assume_role_policy = data.aws_iam_policy_document.dashboard_assume_role.json
  tags               = var.tags
}

data "aws_iam_policy_document" "rds_connect" {
  statement {
    actions   = ["rds-db:connect"]
    resources = ["arn:aws:rds-db:${data.aws_region.current.region}:${data.aws_caller_identity.current.account_id}:dbuser:${var.rds_instance_id}/*"]
  }
}

resource "aws_iam_policy" "rds_connect" {
  name        = "${var.name_prefix}-rds-connect"
  description = "Allow IAM authentication to RDS for reinhardt-cloud workloads."
  policy      = data.aws_iam_policy_document.rds_connect.json
  tags        = var.tags
}

resource "aws_iam_role_policy_attachment" "operator_rds" {
  role       = aws_iam_role.operator.name
  policy_arn = aws_iam_policy.rds_connect.arn
}

resource "aws_iam_role_policy_attachment" "dashboard_rds" {
  role       = aws_iam_role.dashboard.name
  policy_arn = aws_iam_policy.rds_connect.arn
}
