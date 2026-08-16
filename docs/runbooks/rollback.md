# Release rollback

Stop the canary automatically when readiness fails, 5xx exceeds 1% for five
minutes, or p95 exceeds 500 ms. Drain the new tasks, restore the previous
approved API/worker/frontend image digests, and verify `/health/live`,
`/health/ready`, synthetic login and one read-only leaderboard request.

Migrations are forward-only and must remain compatible with the previous image
during the rollout window. If they are not, stop promotion and restore an
isolated encrypted backup; never edit or roll back an applied migration. Redis
is rebuilt from PostgreSQL. Record the stop reason, UTC timestamps, image
digests, migration version, operator, and follow-up owner.
