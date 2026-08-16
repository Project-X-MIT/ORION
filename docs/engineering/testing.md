# Performance and failure evidence

The performance harness uses k6 scripts in `tests/performance/` and SQL
fixtures that create only synthetic users, ratings, papers and reviews. The
launch model is 20 VUs for peak API traffic and 50 leaderboard requests per
second for 30 seconds, with 2x headroom. Gates are p95 <500 ms, p99 <1 s,
error rate <1%, and no freshness/queue growth.

Run locally with `k6 run tests/performance/api_load.js` and
`k6 run tests/performance/leaderboard.js` after starting the Compose stack.
Run the SQL fixtures against an isolated database with `psql -f`; never point
them at production. Record the k6 summary, database query plans, CPU/memory,
connection count, pending outbox count and Redis rebuild result as release
evidence.

The synthetic PostgreSQL run on 2026-08-14 completed both fixtures without
errors: 100,000 ranked users (deep-page query 88.8 ms) and 250,000 papers plus
500,000 reviews (point lookup 0.012 ms; aggregate 85.5 ms). These are local
planner timings, not a production SLO sign-off; repeat the k6 scenario in the
provider staging environment before canary approval.

For a repeatable disposable-stack drill covering health, metrics, both k6
scenarios, Redis loss, PostgreSQL mutation failure, worker restart and API
shutdown, run `RUN_ID=linux-staging scripts/staging-smoke.sh`. The script
writes a timestamped report under `.staging-evidence/` (ignored by Git).

Failure matrix: Redis is stopped and cache reads must fall back to PostgreSQL;
PostgreSQL is stopped and mutations must fail before acceptance; a worker is
terminated during dispatch and its lease must be recovered; duplicate events
must remain one business effect. Provider timeouts use bounded retry and
dead-letter state. Restore the synthetic database after each run.

## Local staging observability profile

The local Compose stack has an opt-in `monitoring` profile for Linux staging
validation. It starts Prometheus, Alertmanager, Grafana and a synthetic webhook
receiver; the receiver records only alert status, name, owner and runbook.

```bash
docker compose -f infra/compose/docker-compose.yml --profile monitoring up -d
docker compose -f infra/compose/docker-compose.yml exec -T api \
  curl --fail http://127.0.0.1:3000/metrics
curl --fail http://127.0.0.1:9090/-/ready
curl --fail http://127.0.0.1:9093/-/ready
curl --fail http://127.0.0.1:3300/api/health
```

The API metrics are read-only PostgreSQL snapshots. `orion_outbox_pending_events`
counts pending durable events, while
`orion_rating_reconciliation_failures_total` compares each current rating with
the latest append-only ledger value (or the 1200 starting rating). A local
divergence drill may insert a synthetic mismatch, verify that the metric and
alert reach the webhook, and then restore the isolated database; it must never
run against production.
