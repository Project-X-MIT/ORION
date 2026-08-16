# Release rollback

Stop the canary automatically when readiness fails, 5xx exceeds 1% for five
minutes, or p95 exceeds 500 ms. Drain the new tasks, restore the previous
approved API/worker/frontend image digests, and verify `/health/live`,
`/health/ready`, synthetic login and one read-only leaderboard request.

The automated readiness gate is `tools/canary-gate.sh`. It calls the protected
rollback hook with the previous immutable digests as soon as `/health/ready`
fails and fails closed when the hook does not acknowledge. The workflow records
the elapsed time and enforces the configured recovery target (30 seconds in the
protected release workflow). The hook must stop traffic before an operator
retries promotion.

Migrations are forward-only and must remain compatible with the previous image
during the rollout window. If they are not, stop promotion and restore an
isolated encrypted backup; never edit or roll back an applied migration. Redis
is rebuilt from PostgreSQL. Record the stop reason, UTC timestamps, image
digests, migration version, operator, and follow-up owner.

After a successful rollback, run `tools/post-deploy-smoke.sh` against the edge
URL and attach its output to the sign-off record. The smoke script verifies the
live/readiness endpoints and the public edge response without logging secrets.
