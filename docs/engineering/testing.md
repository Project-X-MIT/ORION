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

Failure matrix: Redis is stopped and cache reads must fall back to PostgreSQL;
PostgreSQL is stopped and mutations must fail before acceptance; a worker is
terminated during dispatch and its lease must be recovered; duplicate events
must remain one business effect. Provider timeouts use bounded retry and
dead-letter state. Restore the synthetic database after each run.
