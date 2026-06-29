resource "google_service_account" "nodes" {
  project      = var.project_id
  account_id   = "${var.name_prefix}-nodes"
  display_name = "Reinhardt Cloud GKE node pool service account"
}

resource "google_project_iam_member" "nodes_log_writer" {
  project = var.project_id
  role    = "roles/logging.logWriter"
  member  = "serviceAccount:${google_service_account.nodes.email}"
}

resource "google_project_iam_member" "nodes_metric_writer" {
  project = var.project_id
  role    = "roles/monitoring.metricWriter"
  member  = "serviceAccount:${google_service_account.nodes.email}"
}

resource "google_project_iam_member" "nodes_monitoring_viewer" {
  project = var.project_id
  role    = "roles/monitoring.viewer"
  member  = "serviceAccount:${google_service_account.nodes.email}"
}

resource "google_container_cluster" "primary" {
  project  = var.project_id
  name     = "${var.name_prefix}-cluster"
  location = var.zone

  # Remove default node pool; managed node pool defined below.
  remove_default_node_pool = true
  initial_node_count       = 1

  network    = var.network_id
  subnetwork = var.subnet_id

  ip_allocation_policy {
    cluster_secondary_range_name  = var.pods_range_name
    services_secondary_range_name = var.services_range_name
  }

  # Dataplane V2 enforces Kubernetes NetworkPolicy resources for tenant isolation.
  # It is replacement-only for existing GKE clusters, so keep it explicit.
  datapath_provider = var.enable_dataplane_v2 ? "ADVANCED_DATAPATH" : null

  private_cluster_config {
    enable_private_nodes    = true
    enable_private_endpoint = false
    master_ipv4_cidr_block  = "172.16.0.0/28"
  }

  workload_identity_config {
    workload_pool = "${var.project_id}.svc.id.goog"
  }

  # Enable Binary Authorization for supply-chain security.
  binary_authorization {
    evaluation_mode = "PROJECT_SINGLETON_POLICY_ENFORCE"
  }

  resource_labels = var.labels
}

resource "google_container_node_pool" "primary" {
  project    = var.project_id
  name       = "${var.name_prefix}-nodes"
  location   = var.zone
  cluster    = google_container_cluster.primary.name
  node_count = var.node_count

  node_config {
    machine_type    = var.machine_type
    disk_size_gb    = var.disk_size_gb
    service_account = google_service_account.nodes.email

    oauth_scopes = [
      "https://www.googleapis.com/auth/cloud-platform",
    ]

    workload_metadata_config {
      mode = "GKE_METADATA"
    }

    labels = var.labels
  }

  management {
    auto_repair  = true
    auto_upgrade = true
  }
}
