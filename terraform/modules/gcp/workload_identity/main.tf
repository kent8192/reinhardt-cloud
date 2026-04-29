resource "google_service_account" "operator" {
  project      = var.project_id
  account_id   = "${var.name_prefix}-operator"
  display_name = "Reinhardt Cloud operator GSA"
}

resource "google_service_account" "dashboard" {
  project      = var.project_id
  account_id   = "${var.name_prefix}-dashboard"
  display_name = "Reinhardt Cloud dashboard GSA"
}

resource "google_project_iam_member" "operator_sql_client" {
  project = var.project_id
  role    = "roles/cloudsql.client"
  member  = "serviceAccount:${google_service_account.operator.email}"
}

resource "google_project_iam_member" "dashboard_sql_client" {
  project = var.project_id
  role    = "roles/cloudsql.client"
  member  = "serviceAccount:${google_service_account.dashboard.email}"
}

resource "google_service_account_iam_member" "operator_wi_binding" {
  service_account_id = google_service_account.operator.name
  role               = "roles/iam.workloadIdentityUser"
  member             = "serviceAccount:${var.cluster_project_id}.svc.id.goog[${var.operator_namespace}/${var.operator_ksa_name}]"
}

resource "google_service_account_iam_member" "dashboard_wi_binding" {
  service_account_id = google_service_account.dashboard.name
  role               = "roles/iam.workloadIdentityUser"
  member             = "serviceAccount:${var.cluster_project_id}.svc.id.goog[${var.dashboard_namespace}/${var.dashboard_ksa_name}]"
}
