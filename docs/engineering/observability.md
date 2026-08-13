# Observability and data minimization

ORION uses structured JSON logs for operational events. HTTP tracing records
only the request method, validated request ID, response status, and timing;
the URI query string, headers, cookies, authorization values, and request or
response bodies are excluded.

Authentication events correlate with the request ID rather than logging email,
username, session IDs, or other user identifiers. Passwords, password hashes,
database and Redis URLs, API keys, report contents, notification bodies, and
raw request payloads must never be emitted to logs, metrics, or traces.

Errors exposed to clients and operational logs use stable generic messages;
the underlying database or session error is not serialized into an API
response. Add a field only when it is required to diagnose the operation and
is not personal or secret data.

## Research review worker

The worker job body is an orchestration boundary: research eligibility,
evaluation validation, idempotency, and outbox persistence execute in the
database feature transaction. Worker telemetry covers the delivery lifecycle
without logging research content or duplicating transaction decisions.

`orion-worker` exposes process-local research review metrics through
`ResearchReviewJobMetrics::snapshot`. Export these names without adding paper,
user, or payload labels:

- `orion_research_review_jobs_enqueued_total`
- `orion_research_review_jobs_claimed_total`
- `orion_research_review_jobs_completed_total`
- `orion_research_review_jobs_failures_total`
- `orion_research_review_jobs_retries_total`
- `orion_research_review_jobs_dead_letter_total`
- `orion_research_review_job_duration_ms_total`
- `orion_research_review_job_duration_ms_count`
- `orion_research_review_job_duration_ms_max`

Lifecycle traces use the `orion.worker` target. Retry and dead-letter alert
events use the `orion.alerts` target with stable fields:

- `alert=research_review_retry_scheduled`, `severity=warning`
- `alert=research_review_dead_letter`, `severity=critical`

Alert on any increase in `dead_letter_total`; use retry growth and duration
percentiles for early-warning thresholds. Alert events include only the event
identifier, attempt counts, retry delay, and bounded reason metadata.

`job_error` and `dead_letter_reason` store allowlisted failure categories and
retry-budget context only, such as `outbox_persistence_failed`,
`dependency_timeout`, or `invalid_research_evaluation`. They never store raw
errors, SQL details, report content, review text, or user identifiers.
