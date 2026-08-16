#!/usr/bin/env bash
set -euo pipefail

: "${SMOKE_BASE_URL:?SMOKE_BASE_URL is required}"
base_url="${SMOKE_BASE_URL%/}"
report="${SMOKE_REPORT:-}"

[[ "$base_url" =~ ^https?://[^[:space:]]+$ ]] || { echo 'SMOKE_BASE_URL must be an http(s) URL' >&2; exit 2; }

if [[ -n "$report" ]]; then
  mkdir -p "$(dirname "$report")"
  exec > >(tee "$report") 2>&1
fi

check() {
  local path="$1" expected="${2:-2[0-9][0-9]}" status
  status="$(curl --silent --show-error --max-time 10 -o /dev/null -w '%{http_code}' "$base_url$path" || true)"
  echo "smoke_check path=$path status=$status"
  [[ "$status" =~ ^$expected$ ]] || {
    echo "smoke check failed for $path" >&2
    exit 1
  }
}

check /health/live
check /health/ready
check /
echo "post_deploy_smoke=pass"
