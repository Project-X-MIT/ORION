# API degradation

**Owner:** platform on-call · **Severity:** page · **SLO:** 99.9% availability,
p95 request latency under 500 ms.

1. Confirm `/health/live`, `/health/ready`, `up{job="orion-api"}` and the
   request error/latency panels. Do not inspect request bodies or cookies.
2. Check PostgreSQL and Redis health. PostgreSQL is authoritative; Redis may be
   restarted or rebuilt without accepting an uncommitted business mutation.
3. Stop rollout, drain the affected API tasks, and restore the last known-good
   immutable image if the error rate remains above the SLO for five minutes.
4. Verify both health endpoints and a synthetic login/quiz read. Attach only
   request IDs, timestamps and aggregate metrics to the incident.
