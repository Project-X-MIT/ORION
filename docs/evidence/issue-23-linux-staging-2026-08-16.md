# DIV-14 load, soak and failure evidence

Run date: 2026-08-16  
Commit: `30d6266fec92aa4ca7c84ff6b71414b77efb6e4c`  
Environment: disposable synthetic Compose stack on `orion-vm` (Ubuntu),
`orion-local`; no production data or credentials.

## Reproduction

```bash
RUN_ID=issue23-linux-staging ORION_BASE_URL=http://127.0.0.1:5173 \
  scripts/staging-smoke.sh
```

The command writes the full timestamped smoke report to
`.staging-evidence/` (ignored). The soak was ten consecutive runs of
`tests/performance/leaderboard.js` at 50 requests/second for 30 seconds each:

```bash
for i in $(seq 1 10); do
  docker run --rm --network host \
    -e ORION_BASE_URL=http://127.0.0.1:5173 \
    -v "$PWD/tests/performance:/scripts:ro" grafana/k6:latest \
    run /scripts/leaderboard.js
done
```

## Results

| Check | Result |
| --- | --- |
| Leaderboard peak/headroom | 1,500 requests at 50 req/s; 0% errors; p95 3.18 ms; p99 5.97 ms |
| API peak/headroom | 2,509 iterations / 5,018 HTTP requests at 20 VUs; 0% errors; p95 182.5 ms; p99 201.98 ms |
| Soak | 14,960 requests over 5 minutes; every run 0% errors; worst p99 42.01 ms |
| Redis loss | Leaderboard remained HTTP 200; Redis restart returned the API to healthy |
| PostgreSQL loss | Register mutation returned HTTP 503; no accepted mutation; database restart returned healthy |
| Worker and API restart | Worker health returned healthy; API graceful shutdown and readiness succeeded |
| Network partition | Worker disconnected from `orion-local_backend` for 10 seconds, then reconnected and returned healthy |
| Memory observation | API 2.47 MiB -> 6.83 MiB and PostgreSQL 37.72 MiB -> 55.59 MiB across the soak; no monotonic/unbounded trend was observed |

The approved gates are p95 <500 ms, p99 <1 s, error rate <1%, and no
freshness/queue growth. The peak model retains 2x headroom. Scale out the API
when either latency or error gates are approached; scale workers when pending
outbox/queue depth grows, and repeat the same synthetic run after scaling.

## Duplicate, timeout and provider retry coverage

The successful CI run for this commit executed `cargo test --workspace
--locked`, including:

- `one_hundred_duplicate_deliveries_create_one_settlement_and_rating_event`
- `duplicate_concurrent_settlement_is_idempotent`
- `outbox_claim_lease_retry_dead_letter_and_replay_preserve_identity`
- `provider_outage_retries_boundedly_and_leaves_attempt_pending`

These tests verify one business effect for duplicate delivery, bounded retry
and durable dead-letter behavior for provider failure, and lease recovery for
timeouts/retries. CI also passed Rust format/clippy, frontend lint/type-check/
tests/build, Compose smoke, and security checks.

The smoke report and CI run remain the release evidence; production SLO
approval and canary sign-off are intentionally not inferred from this
synthetic staging run.
