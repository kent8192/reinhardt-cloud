#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

APP_NAME="${DASHBOARD_SELF_DEPLOY_APP_NAME:-reinhardt-cloud-dashboard}"
IMAGE="${DASHBOARD_SELF_DEPLOY_IMAGE:-reinhardt-cloud-dashboard:e2e}"
PROJECT_DIR="${DASHBOARD_SELF_DEPLOY_PROJECT_DIR:-${ROOT_DIR}/dashboard}"
NAMESPACE="${DASHBOARD_SELF_DEPLOY_NAMESPACE:-reinhardt-dashboard-e2e-$(date +%s)}"
ARTIFACT_DIR="${DASHBOARD_SELF_DEPLOY_ARTIFACT_DIR:-${ROOT_DIR}/target/dashboard-self-deploy-e2e/${NAMESPACE}}"
TIMEOUT="${DASHBOARD_SELF_DEPLOY_TIMEOUT:-180s}"
INTROSPECT_TIMEOUT_SECONDS="${DASHBOARD_SELF_DEPLOY_INTROSPECT_TIMEOUT_SECONDS:-30}"
BUILD_IMAGE="${DASHBOARD_SELF_DEPLOY_BUILD_IMAGE:-1}"
KEEP_RESOURCES="${DASHBOARD_SELF_DEPLOY_KEEP_RESOURCES:-0}"
OPERATOR_MODE="${DASHBOARD_SELF_DEPLOY_OPERATOR_MODE:-auto}"
OPERATOR_METRICS_ADDR="${DASHBOARD_SELF_DEPLOY_OPERATOR_METRICS_ADDR:-127.0.0.1:19090}"
OPERATOR_BIN="${DASHBOARD_SELF_DEPLOY_OPERATOR_BIN:-${ROOT_DIR}/target/debug/reinhardt-cloud-operator}"
CLI_BIN="${DASHBOARD_SELF_DEPLOY_CLI_BIN:-${ROOT_DIR}/target/debug/reinhardt-cloud}"
MANAGE_BIN="${DASHBOARD_SELF_DEPLOY_MANAGE_BIN:-${ROOT_DIR}/target/debug/manage}"
MANAGE_ENV="${DASHBOARD_SELF_DEPLOY_REINHARDT_ENV:-${REINHARDT_ENV:-ci}}"
RUNTIME_SECRET="${DASHBOARD_SELF_DEPLOY_RUNTIME_SECRET:-reinhardt-cloud-dashboard-secrets}"
KUBECTL_CONTEXT="${DASHBOARD_SELF_DEPLOY_KUBECTL_CONTEXT:-}"
KIND_CLUSTER="${DASHBOARD_SELF_DEPLOY_KIND_CLUSTER:-}"
E2E_USERNAME="${DASHBOARD_SELF_DEPLOY_E2E_USERNAME:-e2e-user}"
E2E_PASSWORD="${DASHBOARD_SELF_DEPLOY_E2E_PASSWORD:-e2e-password-123456}"
E2E_EMAIL="${DASHBOARD_SELF_DEPLOY_E2E_EMAIL:-e2e@example.test}"
PORT_FORWARD_PORT="${DASHBOARD_SELF_DEPLOY_PORT_FORWARD_PORT:-18080}"
E2E_ORIGIN="${DASHBOARD_SELF_DEPLOY_ORIGIN:-http://127.0.0.1:8000}"

OPERATOR_PID=""
PORT_FORWARD_PID=""
OPERATOR_LOG="${ARTIFACT_DIR}/operator.log"
PORT_FORWARD_LOG="${ARTIFACT_DIR}/port-forward.log"
DRY_RUN_YAML="${ARTIFACT_DIR}/reinhardt-app.yaml"
CREATED_NAMESPACE=0

kubectl_args=()
cli_cluster_args=()
if [[ -n "${KUBECTL_CONTEXT}" ]]; then
	kubectl_args+=(--context "${KUBECTL_CONTEXT}")
	cli_cluster_args+=(--cluster "${KUBECTL_CONTEXT}")
fi

log() {
	printf '[dashboard-self-deploy-e2e] %s\n' "$*"
}

die() {
	printf '[dashboard-self-deploy-e2e] ERROR: %s\n' "$*" >&2
	exit 1
}

command_exists() {
	command -v "$1" >/dev/null 2>&1
}

kubectl_cmd() {
	kubectl "${kubectl_args[@]}" "$@"
}

collect_diagnostics() {
	mkdir -p "${ARTIFACT_DIR}"
	log "collecting diagnostics in ${ARTIFACT_DIR}"

	kubectl_cmd get namespace "${NAMESPACE}" -o yaml >"${ARTIFACT_DIR}/namespace.yaml" 2>&1 || true
	kubectl_cmd get reinhardtapp "${APP_NAME}" -n "${NAMESPACE}" -o yaml >"${ARTIFACT_DIR}/reinhardtapp-live.yaml" 2>&1 || true
	kubectl_cmd describe reinhardtapp "${APP_NAME}" -n "${NAMESPACE}" >"${ARTIFACT_DIR}/reinhardtapp-describe.txt" 2>&1 || true
	kubectl_cmd get all,secrets,configmaps,pvc -n "${NAMESPACE}" -o wide >"${ARTIFACT_DIR}/namespace-resources.txt" 2>&1 || true
	kubectl_cmd get events -n "${NAMESPACE}" --sort-by=.lastTimestamp >"${ARTIFACT_DIR}/events.txt" 2>&1 || true
	kubectl_cmd get deployment "${APP_NAME}" -n "${NAMESPACE}" -o yaml >"${ARTIFACT_DIR}/deployment-live.yaml" 2>&1 || true
	kubectl_cmd get deployment,service,statefulset,job,pod -n "${NAMESPACE}" \
		-l "app.kubernetes.io/name=${APP_NAME}" -o yaml >"${ARTIFACT_DIR}/owned-resources.yaml" 2>&1 || true
	kubectl_cmd logs -n "${NAMESPACE}" \
		-l "app.kubernetes.io/name=${APP_NAME}" --all-containers --tail=250 --prefix >"${ARTIFACT_DIR}/owned-pod-logs.txt" 2>&1 || true
	kubectl_cmd logs -A \
		-l "app.kubernetes.io/name=reinhardt-cloud-operator" --all-containers --tail=250 --prefix >"${ARTIFACT_DIR}/cluster-operator-logs.txt" 2>&1 || true
}

stop_process_tree() {
	local pid=$1
	local children

	children="$(pgrep -P "${pid}" 2>/dev/null || true)"
	for child in ${children}; do
		stop_process_tree "${child}"
	done

	kill "${pid}" >/dev/null 2>&1 || true
}

cleanup() {
	local status=$?

	if [[ "${status}" -ne 0 ]]; then
		collect_diagnostics
	fi

	if [[ -n "${OPERATOR_PID}" ]]; then
		log "stopping local operator process ${OPERATOR_PID}"
		stop_process_tree "${OPERATOR_PID}"
		wait "${OPERATOR_PID}" >/dev/null 2>&1 || true
	fi

	if [[ -n "${PORT_FORWARD_PID}" ]]; then
		log "stopping port-forward process ${PORT_FORWARD_PID}"
		stop_process_tree "${PORT_FORWARD_PID}"
		wait "${PORT_FORWARD_PID}" >/dev/null 2>&1 || true
	fi

	if [[ "${KEEP_RESOURCES}" == "1" ]]; then
		log "keeping namespace ${NAMESPACE} and artifacts ${ARTIFACT_DIR}"
		exit "${status}"
	fi

	if [[ "${CREATED_NAMESPACE}" == "1" ]]; then
		log "deleting namespace ${NAMESPACE}"
		kubectl_cmd delete namespace "${NAMESPACE}" --ignore-not-found --wait=false >/dev/null 2>&1 || true
	else
		kubectl_cmd delete reinhardtapp "${APP_NAME}" -n "${NAMESPACE}" --ignore-not-found >/dev/null 2>&1 || true
	fi

	exit "${status}"
}
trap cleanup EXIT

require_prerequisites() {
	command_exists docker || die "docker is required"
	command_exists kubectl || die "kubectl is required"
	command_exists cargo || die "cargo is required"
	command_exists curl || die "curl is required"

	docker info >/dev/null || die "Docker is not reachable"
	kubectl_cmd version --client >/dev/null || die "kubectl client is not usable"
	kubectl_cmd cluster-info >/dev/null || die "Kubernetes cluster is not reachable"
}

ensure_namespace() {
	if kubectl_cmd get namespace "${NAMESPACE}" >/dev/null 2>&1; then
		log "using existing namespace ${NAMESPACE}"
		return
	fi

	kubectl_cmd create namespace "${NAMESPACE}"
	CREATED_NAMESPACE=1
}

ensure_crd() {
	log "applying ReinhardtApp CRD"
	kubectl_cmd apply -f "${ROOT_DIR}/charts/reinhardt-cloud-operator/crds/reinhardtapp-crd.yaml"
	kubectl_cmd wait --for=condition=Established crd/reinhardtapps.paas.reinhardt-cloud.dev --timeout="${TIMEOUT}"
}

ensure_runtime_secret() {
	log "creating temporary runtime secret ${RUNTIME_SECRET}"
	kubectl_cmd create secret generic "${RUNTIME_SECRET}" -n "${NAMESPACE}" \
		--from-literal=email-host=localhost \
		--dry-run=client -o yaml | kubectl_cmd apply -f -

	if [[ ! -f "${PROJECT_DIR}/reinhardt-cloud.toml" ]]; then
		return
	fi

	sed -n 's/.*secretRef:\([^"[:space:]]*\).*/\1/p' "${PROJECT_DIR}/reinhardt-cloud.toml" \
		| sort -u \
		| while IFS=/ read -r secret_name secret_key; do
			if [[ -z "${secret_name}" || -z "${secret_key}" ]]; then
				continue
			fi
			local secret_value="e2e-placeholder"
			if [[ "${secret_name}" == "${RUNTIME_SECRET}" && "${secret_key}" == "email-host" ]]; then
				secret_value="localhost"
			fi
			kubectl_cmd create secret generic "${secret_name}" -n "${NAMESPACE}" \
				--from-literal=placeholder=unused \
				--dry-run=client -o yaml | kubectl_cmd apply -f -
			kubectl_cmd patch secret "${secret_name}" -n "${NAMESPACE}" \
				--type merge \
				-p "{\"stringData\":{\"${secret_key}\":\"${secret_value}\"}}" >/dev/null
		done
}

build_dashboard_image() {
	if [[ "${BUILD_IMAGE}" != "1" ]]; then
		log "using prebuilt dashboard image ${IMAGE}"
		return
	fi

	log "building dashboard image ${IMAGE}"
	docker build -f "${ROOT_DIR}/dashboard/Dockerfile" -t "${IMAGE}" "${ROOT_DIR}"

	if [[ -n "${KIND_CLUSTER}" ]]; then
		command_exists kind || die "kind is required when DASHBOARD_SELF_DEPLOY_KIND_CLUSTER is set"
		log "loading ${IMAGE} into kind cluster ${KIND_CLUSTER}"
		kind load docker-image "${IMAGE}" --name "${KIND_CLUSTER}"
		return
	fi

	local current_context
	current_context="$(kubectl_cmd config current-context 2>/dev/null || true)"
	if [[ "${current_context}" == kind-* ]] && command_exists kind; then
		log "loading ${IMAGE} into kind cluster ${current_context#kind-}"
		kind load docker-image "${IMAGE}" --name "${current_context#kind-}"
	fi
}

build_local_binaries() {
	local packages=()
	local build_manage=0

	if [[ -z "${DASHBOARD_SELF_DEPLOY_CLI_BIN:-}" ]]; then
		packages+=(-p reinhardt-cloud-cli)
	else
		log "using CLI binary ${CLI_BIN}"
	fi

	if [[ -z "${DASHBOARD_SELF_DEPLOY_MANAGE_BIN:-}" ]]; then
		build_manage=1
	else
		log "using dashboard manage binary ${MANAGE_BIN}"
	fi

	if [[ -z "${DASHBOARD_SELF_DEPLOY_OPERATOR_BIN:-}" && "${OPERATOR_MODE}" != "existing" && "${OPERATOR_MODE}" != "skip" ]]; then
		packages+=(-p reinhardt-cloud-operator)
	elif [[ -n "${DASHBOARD_SELF_DEPLOY_OPERATOR_BIN:-}" ]]; then
		log "using operator binary ${OPERATOR_BIN}"
	fi

	if [[ "${#packages[@]}" -ne 0 ]]; then
		log "building local cloud binaries"
		(
			cd "${ROOT_DIR}"
			cargo build --locked "${packages[@]}"
		)
	fi

	if [[ "${build_manage}" == "1" ]]; then
		log "building dashboard manage binary"
		(
			cd "${ROOT_DIR}"
			cargo build --locked -p reinhardt-cloud-dashboard --bin manage
		)
	fi
}

operator_deployment_exists() {
	kubectl_cmd get deployment -A \
		-l "app.kubernetes.io/name=reinhardt-cloud-operator" \
		-o name 2>/dev/null | grep -q .
}

ensure_operator() {
	case "${OPERATOR_MODE}" in
		existing)
			operator_deployment_exists || die "no in-cluster operator deployment was found"
			log "using existing in-cluster operator"
			;;
		local)
			start_local_operator
			;;
		skip)
			log "skipping operator startup check"
			;;
		auto)
			if operator_deployment_exists; then
				log "using existing in-cluster operator"
			else
				start_local_operator
			fi
			;;
		*)
			die "invalid DASHBOARD_SELF_DEPLOY_OPERATOR_MODE=${OPERATOR_MODE}; expected auto, existing, local, or skip"
			;;
	esac
}

start_local_operator() {
	mkdir -p "${ARTIFACT_DIR}"
	log "starting local operator; logs: ${OPERATOR_LOG}"
	(
		cd "${ROOT_DIR}"
		REINHARDT_CLOUD_METRICS_ADDR="${OPERATOR_METRICS_ADDR}" "${OPERATOR_BIN}"
	) >"${OPERATOR_LOG}" 2>&1 &
	OPERATOR_PID=$!
	sleep 3
	if ! kill -0 "${OPERATOR_PID}" >/dev/null 2>&1; then
		die "local operator exited during startup; see ${OPERATOR_LOG}"
	fi
}

generate_reinhardt_app_yaml() {
	mkdir -p "${ARTIFACT_DIR}"
	log "generating dry-run ReinhardtApp YAML"
	(
		cd "${ROOT_DIR}"
		export REINHARDT_ENV="${MANAGE_ENV}"
		"${CLI_BIN}" deploy \
			--dir "${PROJECT_DIR}" \
			--name "${APP_NAME}" \
			--image "${IMAGE}" \
			--namespace "${NAMESPACE}" \
			--manage-bin "${MANAGE_BIN}" \
			--require-introspect \
			--dry-run
	) >"${DRY_RUN_YAML}"

	grep -q '^kind: ReinhardtApp$' "${DRY_RUN_YAML}" || die "dry-run output did not contain a ReinhardtApp manifest"
}

apply_reinhardt_app_direct() {
	log "applying ReinhardtApp through the CLI --direct path"
	(
		cd "${ROOT_DIR}"
		export REINHARDT_ENV="${MANAGE_ENV}"
		"${CLI_BIN}" deploy \
			--dir "${PROJECT_DIR}" \
			--name "${APP_NAME}" \
			--image "${IMAGE}" \
			--namespace "${NAMESPACE}" \
			--manage-bin "${MANAGE_BIN}" \
			--require-introspect \
			"${cli_cluster_args[@]}" \
			--direct
	)
	kubectl_cmd patch reinhardtapp "${APP_NAME}" -n "${NAMESPACE}" --type merge \
		-p "{\"spec\":{\"env\":{\"REINHARDT_ENV\":\"${MANAGE_ENV}\"}}}" >/dev/null
}

wait_for_resource_exists() {
	local kind=$1
	local name=$2
	local deadline
	deadline=$((SECONDS + $(timeout_seconds)))

	log "waiting for ${kind}/${name} to exist"
	while ((SECONDS < deadline)); do
		if kubectl_cmd get "${kind}" "${name}" -n "${NAMESPACE}" >/dev/null 2>&1; then
			return
		fi
		sleep 3
	done

	die "${kind}/${name} was not created before ${TIMEOUT}"
}

FOUND_RESOURCE_NAME=""
wait_for_first_resource_exists() {
	local kind=$1
	shift
	local deadline
	local name
	deadline=$((SECONDS + $(timeout_seconds)))

	log "waiting for one ${kind} to exist: $*"
	while ((SECONDS < deadline)); do
		for name in "$@"; do
			if kubectl_cmd get "${kind}" "${name}" -n "${NAMESPACE}" >/dev/null 2>&1; then
				FOUND_RESOURCE_NAME="${name}"
				return
			fi
		done
		sleep 3
	done

	die "none of ${kind}/$* was created before ${TIMEOUT}"
}

timeout_seconds() {
	case "${TIMEOUT}" in
		*s)
			printf '%s\n' "${TIMEOUT%s}"
			;;
		*m)
			printf '%s\n' "$(( ${TIMEOUT%m} * 60 ))"
			;;
		*)
			printf '%s\n' "${TIMEOUT}"
			;;
	esac
}

wait_for_app_ready_condition() {
	local deadline
	local phase
	local ready
	deadline=$((SECONDS + $(timeout_seconds)))

	log "waiting for ReinhardtApp/${APP_NAME} Ready condition"
	while (( SECONDS < deadline )); do
		phase="$(kubectl_cmd get reinhardtapp "${APP_NAME}" -n "${NAMESPACE}" -o jsonpath='{.status.phase}' 2>/dev/null || true)"
		ready="$(kubectl_cmd get reinhardtapp "${APP_NAME}" -n "${NAMESPACE}" -o jsonpath='{.status.conditions[?(@.type=="Ready")].status}' 2>/dev/null || true)"
		if [[ "${phase}" == "running" && "${ready}" == "True" ]]; then
			return
		fi
		sleep 3
	done

	die "ReinhardtApp/${APP_NAME} did not reach phase=running and Ready=True before ${TIMEOUT}"
}

wait_for_reconciliation() {
	log "waiting for operator-owned resources"
	wait_for_resource_exists deployment "${APP_NAME}"
	kubectl_cmd wait --for=condition=Available "deployment/${APP_NAME}" -n "${NAMESPACE}" --timeout="${TIMEOUT}"
	wait_for_resource_exists service "${APP_NAME}"
	kubectl_cmd get "service/${APP_NAME}" -n "${NAMESPACE}" >/dev/null

	wait_for_resource_exists deployment "${APP_NAME}-redis"
	kubectl_cmd wait --for=condition=Available "deployment/${APP_NAME}-redis" -n "${NAMESPACE}" --timeout="${TIMEOUT}"

	wait_for_first_resource_exists statefulset "${APP_NAME}-db" "${APP_NAME}-postgresql"
	kubectl_cmd wait --for=jsonpath='{.status.readyReplicas}'=1 "statefulset/${FOUND_RESOURCE_NAME}" -n "${NAMESPACE}" --timeout="${TIMEOUT}"
	wait_for_app_ready_condition

	kubectl_cmd get reinhardtapp "${APP_NAME}" -n "${NAMESPACE}" -o yaml >"${ARTIFACT_DIR}/reinhardtapp-ready.yaml"
	kubectl_cmd get deployment,service,statefulset,pod -n "${NAMESPACE}" \
		-l "app.kubernetes.io/name=${APP_NAME}" -o wide >"${ARTIFACT_DIR}/ready-resources.txt"
}

dashboard_pod_name() {
	local deadline
	local pods
	deadline=$((SECONDS + $(timeout_seconds)))

	while (( SECONDS < deadline )); do
		pods="$(kubectl_cmd get pod -n "${NAMESPACE}" \
			-l "app.kubernetes.io/name=${APP_NAME},app.kubernetes.io/component=web" \
			-o go-template='{{range .items}}{{ $name := .metadata.name }}{{ if not .metadata.deletionTimestamp }}{{ range .status.conditions }}{{ if and (eq .type "Ready") (eq .status "True") }}{{ $name }}{{ "\n" }}{{ end }}{{ end }}{{ end }}{{ end }}' \
			2>/dev/null || true)"
		if [[ -n "${pods}" ]]; then
			printf '%s\n' "${pods}" | head -n 1
			return
		fi
		sleep 3
	done
}

seed_authenticated_user() {
	local pod
	pod="$(dashboard_pod_name)"
	if [[ -z "${pod}" ]]; then
		die "could not find dashboard web pod"
	fi

	log "seeding authenticated dashboard user in pod ${pod}"
	kubectl_cmd exec -n "${NAMESPACE}" "${pod}" -c "${APP_NAME}" -- env \
		DASHBOARD_SELF_DEPLOY_E2E_USERNAME="${E2E_USERNAME}" \
		DASHBOARD_SELF_DEPLOY_E2E_PASSWORD="${E2E_PASSWORD}" \
		DASHBOARD_SELF_DEPLOY_E2E_EMAIL="${E2E_EMAIL}" \
		/app/manage seed-self-deploy-user
}

start_dashboard_port_forward() {
	mkdir -p "${ARTIFACT_DIR}"
	log "starting dashboard port-forward on 127.0.0.1:${PORT_FORWARD_PORT}; logs: ${PORT_FORWARD_LOG}"
	kubectl_cmd port-forward -n "${NAMESPACE}" "service/${APP_NAME}" "${PORT_FORWARD_PORT}:80" \
		>"${PORT_FORWARD_LOG}" 2>&1 &
	PORT_FORWARD_PID=$!

	local deadline
	deadline=$((SECONDS + 30))
	while ((SECONDS < deadline)); do
		if curl -fsS "http://127.0.0.1:${PORT_FORWARD_PORT}/api/healthz/" \
			>"${ARTIFACT_DIR}/healthz.json" 2>"${ARTIFACT_DIR}/healthz.err"; then
			return
		fi
		if ! kill -0 "${PORT_FORWARD_PID}" >/dev/null 2>&1; then
			die "dashboard port-forward exited during startup; see ${PORT_FORWARD_LOG}"
		fi
		sleep 1
	done

	die "dashboard health endpoint was not reachable through port-forward"
}

fetch_authenticated_dashboard_page() {
	local base_url="$1"
	local cookie_jar="$2"
	local path="$3"
	local output="$4"
	local marker="$5"
	local status

	status="$(curl -sS -L -o "${output}" -w "%{http_code}" \
		-b "${cookie_jar}" "${base_url}${path}" || true)"
	[[ "${status}" == "200" ]] \
		|| die "authenticated ${path} page returned HTTP ${status}; see ${output}"

	grep -Fq -- "${marker}" "${output}" \
		|| die "authenticated ${path} page did not render ${marker}; see ${output}"
}

verify_authenticated_frontend_flows() {
	local base_url="http://127.0.0.1:${PORT_FORWARD_PORT}"
	local cookie_jar="${ARTIFACT_DIR}/dashboard-cookie.jar"
	local login_body="${ARTIFACT_DIR}/login-response.json"
	local login_error="${ARTIFACT_DIR}/login.err"
	local login_status

	log "verifying authenticated dashboard frontend flows"
	login_status="$(curl -sS -o "${login_body}" -w "%{http_code}" -c "${cookie_jar}" \
		-H "Content-Type: application/x-www-form-urlencoded" \
		-H "Origin: ${E2E_ORIGIN}" \
		-H "Referer: ${E2E_ORIGIN}/login" \
		--data-urlencode "username=${E2E_USERNAME}" \
		--data-urlencode "password=${E2E_PASSWORD}" \
		"${base_url}/api/server_fn/login" \
		2>"${login_error}" \
		|| true)"
	if [[ "${login_status}" != "200" ]]; then
		die "login server function returned HTTP ${login_status}; see ${login_error} and ${login_body}"
	fi

	grep -Eq '"success"[[:space:]]*:[[:space:]]*true' "${login_body}" \
		|| die "login server function did not return success=true; see ${login_body}"

	fetch_authenticated_dashboard_page \
		"${base_url}" "${cookie_jar}" "/clusters/" "${ARTIFACT_DIR}/clusters.html" "Cluster Inventory"
	fetch_authenticated_dashboard_page \
		"${base_url}" "${cookie_jar}" "/deployments/" "${ARTIFACT_DIR}/deployments.html" "Deployment Inventory"
}

main() {
	log "namespace=${NAMESPACE}"
	log "app=${APP_NAME}"
	log "image=${IMAGE}"
	log "manage-env=${MANAGE_ENV}"
	log "artifacts=${ARTIFACT_DIR}"
	export REINHARDT_CLOUD_DEPLOY_INTROSPECT_TIMEOUT_SECONDS="${INTROSPECT_TIMEOUT_SECONDS}"

	require_prerequisites
	ensure_namespace
	ensure_crd
	ensure_runtime_secret
	build_dashboard_image
	build_local_binaries
	ensure_operator
	generate_reinhardt_app_yaml
	apply_reinhardt_app_direct
	wait_for_reconciliation
	seed_authenticated_user
	start_dashboard_port_forward
	verify_authenticated_frontend_flows

	log "completed successfully"
	log "artifacts kept at ${ARTIFACT_DIR}"
}

main "$@"
