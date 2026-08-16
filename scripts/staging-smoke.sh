#!/usr/bin/env bash
set -euo pipefail

# Reproducible synthetic-only staging validation. This script is intentionally
# destructive to the disposable Compose services (not their volumes) and must
# never receive production credentials or URLs.

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
compose=(docker compose -f "$repo_root/infra/compose/docker-compose.yml")
base_url="${ORION_BASE_URL:-http://127.0.0.1:5173}"
run_id="${RUN_ID:-$(date -u +%Y%m%d%H%M%S)}"
run_key="${run_id//[^a-zA-Z0-9]/}"
run_key="${run_key: -8}"
report_dir="${REPORT_DIR:-$repo_root/.staging-evidence}"
report="$report_dir/staging-${run_id}.txt"

mkdir -p "$report_dir"
exec > >(tee "$report") 2>&1

record() {
  printf '[%s] %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*"
}

wait_ready() {
  for _ in $(seq 1 30); do
    if curl --silent --show-error --fail "$base_url/health/ready" >/tmp/orion-ready.json; then
      cat /tmp/orion-ready.json
      return 0
    fi
    sleep 2
  done
  return 1
}

status_code() {
  curl --silent --output /dev/null --write-out '%{http_code}' "$@"
}

cleanup() {
  "${compose[@]}" start redis postgres worker api >/dev/null 2>&1 || true
}
trap cleanup EXIT

record "commit=$(git -C "$repo_root" rev-parse HEAD)"
record "branch=$(git -C "$repo_root" branch --show-current)"
record "compose config validation"
"${compose[@]}" config --quiet

record "start disposable stack"
"${compose[@]}" up --build --detach
wait_ready
record "metrics=$(curl --silent --show-error --fail "$base_url/metrics" | tr '\n' ' ')"
record "redis=$(docker exec orion-local-redis-1 redis-cli --no-auth-warning -a orion-local-redis ping)"

record "leaderboard load test"
docker run --rm --network host \
  -v "$repo_root/tests/performance:/scripts:ro" \
  -e "ORION_BASE_URL=$base_url" \
  -e "RUN_ID=$run_id" \
  grafana/k6:latest run /scripts/leaderboard.js
record "peak API load test"
docker run --rm --network host \
  -v "$repo_root/tests/performance:/scripts:ro" \
  -e "ORION_BASE_URL=$base_url" \
  -e "RUN_ID=$run_id" \
  grafana/k6:latest run /scripts/api_load.js

record "Redis loss fallback"
"${compose[@]}" stop redis
redisless_status="$(status_code "$base_url/api/v1/leaderboard?limit=1")"
record "leaderboard status while Redis is down=$redisless_status"
[[ "$redisless_status" == 200 ]]
"${compose[@]}" start redis
wait_ready

record "PostgreSQL loss fails mutations safely"
"${compose[@]}" stop postgres
mutation_status="$(curl --silent --output /dev/null --write-out '%{http_code}' \
  -H 'Content-Type: application/json' \
  -d "{\"email\":\"staging-${run_id}@synthetic.invalid\",\"username\":\"staging_${run_key}\",\"password\":\"SyntheticLoadPassword123!\"}" \
  "$base_url/api/v1/auth/register" || true)"
record "register status while PostgreSQL is down=$mutation_status"
[[ "$mutation_status" =~ ^(408|500|503)$ ]]
"${compose[@]}" start postgres
wait_ready

record "worker restart after abrupt termination"
worker_id="$("${compose[@]}" ps -q worker)"
docker kill "$worker_id" >/dev/null
"${compose[@]}" start worker
for _ in $(seq 1 20); do
  if docker logs "$worker_id" 2>&1 | grep -q 'orion-worker is ready'; then break; fi
  sleep 2
done
docker logs "$worker_id" 2>&1 | grep -q 'orion-worker is ready'
"${compose[@]}" ps worker

record "API graceful shutdown and restart"
api_id="$("${compose[@]}" ps -q api)"
docker kill --signal TERM "$api_id" >/dev/null
sleep 2
docker logs "$api_id" 2>&1 | tail -20
"${compose[@]}" start api
wait_ready

record "staging evidence complete report=$report"
