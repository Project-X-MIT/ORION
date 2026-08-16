#!/usr/bin/env bash
set -euo pipefail

: "${CANARY_BASE_URL:?CANARY_BASE_URL is required}"
: "${ROLLBACK_HOOK_URL:?ROLLBACK_HOOK_URL is required}"
: "${PREVIOUS_API_IMAGE:?PREVIOUS_API_IMAGE is required}"
: "${PREVIOUS_WORKER_IMAGE:?PREVIOUS_WORKER_IMAGE is required}"
: "${PREVIOUS_FRONTEND_IMAGE:?PREVIOUS_FRONTEND_IMAGE is required}"

window_seconds="${CANARY_WINDOW_SECONDS:-60}"
interval_seconds="${CANARY_INTERVAL_SECONDS:-5}"
recovery_target_seconds="${CANARY_RECOVERY_TARGET_SECONDS:-30}"
base_url="${CANARY_BASE_URL%/}"

[[ "$base_url" =~ ^https?://[^[:space:]]+$ ]] || { echo 'CANARY_BASE_URL must be an http(s) URL' >&2; exit 2; }
[[ "$ROLLBACK_HOOK_URL" =~ ^https?://[^[:space:]]+$ ]] || { echo 'ROLLBACK_HOOK_URL must be an http(s) URL' >&2; exit 2; }
for image in "$PREVIOUS_API_IMAGE" "$PREVIOUS_WORKER_IMAGE" "$PREVIOUS_FRONTEND_IMAGE"; do
  [[ "$image" =~ ^ghcr\.io/[^@]+@sha256:[0-9a-f]{64}$ ]] || {
    echo "previous image is not an immutable GHCR digest: $image" >&2
    exit 2
  }
done
[[ "$window_seconds" =~ ^[1-9][0-9]*$ && "$interval_seconds" =~ ^[1-9][0-9]*$ && "$recovery_target_seconds" =~ ^[1-9][0-9]*$ ]] || {
  echo 'canary timing values must be positive integers' >&2
  exit 2
}

rollback_payload="$(printf '{"api_image":"%s","worker_image":"%s","frontend_image":"%s","reason":"canary_health_gate_failed"}' \
  "$PREVIOUS_API_IMAGE" "$PREVIOUS_WORKER_IMAGE" "$PREVIOUS_FRONTEND_IMAGE")"
rollback_headers=(-H 'Content-Type: application/json')
if [[ -n "${CANARY_ROLLBACK_TOKEN:-}" ]]; then
  rollback_headers+=(-H "Authorization: Bearer $CANARY_ROLLBACK_TOKEN")
fi

started_epoch="$(date +%s)"
rollback() {
  local reason="$1" now elapsed
  now="$(date +%s)"
  elapsed=$((now - started_epoch))
  echo "canary_failure=$reason elapsed_seconds=$elapsed"
  if ! curl --fail --silent --show-error --max-time 10 \
    -X POST "${rollback_headers[@]}" \
    --data "$rollback_payload" "$ROLLBACK_HOOK_URL" >/dev/null; then
    echo 'automatic rollback hook failed; promotion is stopped and manual recovery is required' >&2
    exit 1
  fi
  echo "automatic_rollback=accepted elapsed_seconds=$elapsed"
  if (( elapsed > recovery_target_seconds )); then
    echo "rollback exceeded recovery target (${recovery_target_seconds}s)" >&2
    exit 1
  fi
  exit 1
}

deadline=$((started_epoch + window_seconds))
while (( $(date +%s) < deadline )); do
  probe="$(curl --silent --show-error --max-time 5 -o /dev/null -w '%{http_code} %{time_total}' "$base_url/health/ready" || true)"
  status="${probe%% *}"
  latency="${probe#* }"
  if [[ ! "$status" =~ ^2[0-9][0-9]$ ]]; then
    rollback "readiness_http_${status:-transport_error}"
  fi
  echo "canary_probe status=$status latency_seconds=$latency"
  sleep "$interval_seconds"
done

echo "canary_health_gate=pass window_seconds=$window_seconds"
